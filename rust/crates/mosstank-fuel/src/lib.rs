//! Runs the built `mosstank.wasm` the way the openAC plugin host runs a
//! plugin, and counts the fuel (WebAssembly instructions) every call uses:
//! a performance gate, so that no call may cause lag.
//!
//! The host (openac-rust `crates/ac-plugin`, 4012aeb5) runs plugins on
//! wasmtime with Cranelift and bounds a call by wall time, not fuel (owner
//! decision 38): a call over 100 ms is logged as slow, and only one past
//! 10 s is stopped. Wall time depends on the machine, so this gate keeps
//! the instruction counts as its measure, with the thresholds the host used
//! when it metered fuel: [`FUEL_START`] for instantiation, `enable` and
//! `disable`, [`FUEL_PER_CALL`] for a tick, an event, a callback and the
//! buffer the client hands over before an event, and a page of a large
//! table (`spells_page`) bringing the instructions to decode it, up to
//! `FUEL_PAGED_GRANT_MAX` per call. The gate holds every call to half of
//! that.
//!
//! This runner is the host's loop with wasmtime's fuel metering turned on:
//! the same engine configuration, the same import stubs (suspend, the
//! caller answers, resume), the same grant. What differs is who answers the
//! imports: a [`Client`] of the caller's (the plugin's test client) in
//! place of the session. [`CallFuel::fits`] says whether the call stayed
//! within its threshold.
// The runner is native only: the workspace's WebAssembly build leaves it out.
#![cfg(not(target_arch = "wasm32"))]

use std::collections::BTreeMap;
use std::fmt;

use std::future::Future;
use std::pin::{Pin, pin};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::task::{Context, Poll, Waker};

use ac_plugin_abi::{Import, MODULE, Wire, exports};
use wasmtime::{
    Caller, Config, Engine, Extern, Linker, Memory, Module, ResourceLimiter, Store, TypedFunc,
    WasmParams, WasmResults,
};

/// The instructions one tick, event or command may run (the budget the
/// openac-rust host held calls to when it metered fuel, ef7b128a).
pub const FUEL_PER_CALL: u64 = 100_000_000;

/// The instructions starting (instantiation and `enable`) and stopping may
/// run (the host's budget when it metered fuel, ef7b128a).
pub const FUEL_START: u64 = 1_000_000_000;

/// The host's memory cap when the manifest names none (openac-rust
/// `ac_plugin::manifest::DEFAULT_MEMORY_MIB`).
pub const DEFAULT_MEMORY_MIB: u32 = 256;

/// The fuel a measured call may use: far past any budget, so a call that
/// would fault on the host still finishes and is counted.
const MEASURING_FUEL: u64 = 1_000_000_000_000;

/// Answers the plugin's calls into the client.
pub trait Client {
    /// The encoded reply to one import.
    fn call(&mut self, import: Import, request: &[u8]) -> Vec<u8>;
}

/// What one call into the plugin used.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CallFuel {
    /// The call (`enable`, `tick`, `event`, ...).
    pub call: String,
    /// The host's budget for it.
    pub budget: u64,
    /// The fuel its paged replies brought.
    pub granted: u64,
    /// The instructions it ran.
    pub used: u64,
    /// How often it called each import.
    pub imports: BTreeMap<&'static str, u32>,
    /// Each import in order, with the instructions the plugin ran since
    /// the one before (the call's profile). A log line the plugin writes
    /// that starts with `@` is named by its text: a marker to profile by.
    pub trace: Vec<(std::borrow::Cow<'static, str>, u64)>,
}

impl CallFuel {
    /// What the host would have allowed: the budget and the grants.
    #[must_use]
    pub fn allowed(&self) -> u64 {
        self.budget + self.granted
    }

    /// Whether the call stays within its threshold.
    #[must_use]
    pub fn fits(&self) -> bool {
        self.used <= self.allowed()
    }

    /// The share of the allowed fuel the call used, percent.
    #[must_use]
    pub fn percent(&self) -> f64 {
        #[allow(clippy::cast_precision_loss)]
        let share = self.used as f64 / self.allowed() as f64;
        share * 100.0
    }

    /// Adds another call's use (the buffer an event is handed in, then the
    /// event itself, are one call to the player).
    fn absorb(&mut self, other: &Self) {
        self.used += other.used;
        self.granted += other.granted;
        for (name, count) in &other.imports {
            *self.imports.entry(name).or_default() += count;
        }
    }
}

impl fmt::Display for CallFuel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}: {} instructions of {} ({:.1}%)",
            self.call,
            self.used,
            self.allowed(),
            self.percent()
        )
    }
}

/// What a waiting import asked and the reply it waits for, shared by the
/// import stub (inside the plugin's call) and the runner driving the call:
/// the host's `Exchange`.
#[derive(Debug, Default)]
struct Exchange {
    /// The request, and the fuel left and granted when it was made (for
    /// the profile).
    pending: Option<(Import, Vec<u8>, u64, u64)>,
    reply: Option<Vec<u8>>,
}

fn lock(exchange: &Mutex<Exchange>) -> MutexGuard<'_, Exchange> {
    exchange.lock().unwrap_or_else(PoisonError::into_inner)
}

/// The import stub's wait: ready once the runner has handed the reply over.
struct Reply(Arc<Mutex<Exchange>>);

impl Future for Reply {
    type Output = Vec<u8>;

    fn poll(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Vec<u8>> {
        lock(&self.0)
            .reply
            .take()
            .map_or(Poll::Pending, Poll::Ready)
    }
}

/// The host's memory cap.
#[derive(Debug)]
struct MemoryCap {
    bytes: usize,
}

impl ResourceLimiter for MemoryCap {
    fn memory_growing(
        &mut self,
        _current: usize,
        desired: usize,
        _maximum: Option<usize>,
    ) -> wasmtime::Result<bool> {
        if desired > self.bytes {
            wasmtime::bail!("the plugin's memory cap");
        }
        Ok(true)
    }

    fn table_growing(
        &mut self,
        _current: usize,
        _desired: usize,
        _maximum: Option<usize>,
    ) -> wasmtime::Result<bool> {
        Ok(true)
    }
}

/// The store's data: what an import stub needs during a call.
#[derive(Debug)]
struct StoreData {
    cap: MemoryCap,
    exchange: Arc<Mutex<Exchange>>,
    reply: Vec<u8>,
    panic: Option<String>,
    /// The fuel paged replies granted the running call.
    granted: u64,
}

fn memory_of(caller: &mut Caller<'_, StoreData>) -> wasmtime::Result<Memory> {
    caller
        .get_export(exports::MEMORY)
        .and_then(Extern::into_memory)
        .ok_or_else(|| wasmtime::format_err!("the plugin has no memory"))
}

fn read_request(
    caller: &mut Caller<'_, StoreData>,
    ptr: i32,
    len: i32,
) -> wasmtime::Result<Vec<u8>> {
    let (Ok(offset), Ok(len)) = (usize::try_from(ptr), usize::try_from(len)) else {
        wasmtime::bail!("a request with a negative pointer or length");
    };
    let memory = memory_of(caller)?;
    let mut bytes = vec![0u8; len];
    memory
        .read(&*caller, offset, &mut bytes)
        .map_err(|_| wasmtime::format_err!("a request outside the plugin's memory"))?;
    Ok(bytes)
}

/// Links one import as the host does (`ac_plugin::instance::link`).
fn link(linker: &mut Linker<StoreData>, import: Import) -> wasmtime::Result<()> {
    let name = import.name();
    match import {
        Import::AbiReplyRead => {
            linker.func_wrap(
                MODULE,
                name,
                |mut caller: Caller<'_, StoreData>, ptr: i32, len: i32| -> wasmtime::Result<i32> {
                    let reply = std::mem::take(&mut caller.data_mut().reply);
                    let (Ok(offset), Ok(len)) = (usize::try_from(ptr), usize::try_from(len)) else {
                        wasmtime::bail!("a reply buffer with a negative length");
                    };
                    let take = len.min(reply.len());
                    let memory = memory_of(&mut caller)?;
                    memory
                        .write(&mut caller, offset, &reply[..take])
                        .map_err(|_| {
                            wasmtime::format_err!("a reply buffer outside the plugin's memory")
                        })?;
                    Ok(0)
                },
            )?;
        }
        Import::AbiPanic => {
            linker.func_wrap(
                MODULE,
                name,
                |mut caller: Caller<'_, StoreData>, ptr: i32, len: i32| -> wasmtime::Result<i32> {
                    let bytes = read_request(&mut caller, ptr, len)?;
                    caller.data_mut().panic = Some(String::from_utf8_lossy(&bytes).into_owned());
                    Ok(0)
                },
            )?;
        }
        _ => {
            linker.func_wrap_async(
                MODULE,
                name,
                move |mut caller: Caller<'_, StoreData>,
                      (ptr, len): (i32, i32)|
                      -> Box<dyn Future<Output = wasmtime::Result<i32>> + Send + '_> {
                    Box::new(async move {
                        let request = read_request(&mut caller, ptr, len)?;
                        let left = caller.get_fuel()?;
                        let granted = caller.data().granted;
                        let exchange = Arc::clone(&caller.data().exchange);
                        lock(&exchange).pending = Some((import, request, left, granted));
                        let reply = Reply(exchange).await;
                        // A page of a large table brings the fuel to decode
                        // it, up to a bound per call, as on the host.
                        if let Some(records) = ac_plugin_abi::magic::paged_records(import, &reply)
                        {
                            let granted = caller.data().granted;
                            let grant = (records as u64)
                                .saturating_mul(ac_plugin_abi::magic::FUEL_PER_PAGED_RECORD)
                                .min(ac_plugin_abi::magic::FUEL_PAGED_GRANT_MAX - granted);
                            caller.data_mut().granted = granted + grant;
                            let left = caller.get_fuel()?;
                            caller.set_fuel(left.saturating_add(grant))?;
                        }
                        let len = i32::try_from(reply.len())
                            .map_err(|_| wasmtime::format_err!("a reply larger than 2 GiB"))?;
                        caller.data_mut().reply = reply;
                        Ok(len)
                    })
                },
            )?;
        }
    }
    Ok(())
}

/// The engine the host runs plugins on (`ac_plugin::PluginEngine::new`):
/// wasmtime, Cranelift, fuel metering, each module compiled on all cores.
#[must_use]
pub fn engine() -> Engine {
    let mut config = Config::new();
    config.consume_fuel(true).parallel_compilation(true);
    Engine::new(&config).expect("wasmtime runs on this computer")
}

/// The built plugin, instantiated.
#[allow(missing_debug_implementations)]
pub struct Plugin {
    store: Store<StoreData>,
    exchange: Arc<Mutex<Exchange>>,
    memory: Memory,
    alloc: TypedFunc<i32, i32>,
    enable: TypedFunc<(i32, i32), i64>,
    disable: TypedFunc<(), ()>,
    tick: TypedFunc<(), ()>,
    event: TypedFunc<(i32, i32), ()>,
    callback: Option<TypedFunc<(i32, i32), i64>>,
}

/// A client that answers nothing: instantiation runs before the plugin may
/// call the client.
struct NoClient;

impl Client for NoClient {
    fn call(&mut self, _import: Import, _request: &[u8]) -> Vec<u8> {
        Vec::new()
    }
}

impl Plugin {
    /// Compiles `wasm` with the host's engine.
    ///
    /// # Errors
    /// The module does not compile.
    pub fn compile(engine: &Engine, wasm: &[u8]) -> Result<Module, String> {
        Module::new(engine, wasm).map_err(|error| error.to_string())
    }

    /// Instantiates `wasm` with every import it asks for linked (the host
    /// links only the permitted ones; MossTank's manifest permits all it
    /// uses). The fuel instantiation used is the first measured call.
    ///
    /// # Errors
    /// The module does not load or link, or its start traps.
    pub fn load(wasm: &[u8], memory_mib: u32) -> Result<(Self, CallFuel), String> {
        let engine = engine();
        let module = Self::compile(&engine, wasm)?;
        Self::instantiate(&engine, &module, memory_mib)
    }

    /// [`Self::load`] of a module compiled already.
    ///
    /// # Errors
    /// As [`Self::load`].
    pub fn instantiate(
        engine: &Engine,
        module: &Module,
        memory_mib: u32,
    ) -> Result<(Self, CallFuel), String> {
        let mut linker = Linker::<StoreData>::new(engine);
        for import in module.imports() {
            let import = Import::from_name(import.name())
                .ok_or_else(|| format!("an unknown import {}", import.name()))?;
            link(&mut linker, import).map_err(|error| error.to_string())?;
        }
        let exchange = Arc::new(Mutex::new(Exchange::default()));
        let bytes = usize::try_from(memory_mib).unwrap_or(usize::MAX) << 20;
        let mut store = Store::new(
            engine,
            StoreData {
                cap: MemoryCap { bytes },
                exchange: Arc::clone(&exchange),
                reply: Vec::new(),
                panic: None,
                granted: 0,
            },
        );
        store.limiter(|data| &mut data.cap);
        store
            .set_fuel(MEASURING_FUEL)
            .map_err(|error| error.to_string())?;
        let mut started = CallFuel {
            call: "instantiate".to_owned(),
            budget: FUEL_START,
            ..CallFuel::default()
        };
        let mut traced = 0;
        let instance = drive(
            linker.instantiate_async(&mut store, module),
            &exchange,
            &mut started,
            &mut traced,
            &mut NoClient,
        )?
        .map_err(|error| error.to_string())?;
        let left = store.get_fuel().map_err(|error| error.to_string())?;
        started.used = MEASURING_FUEL - left;
        let memory = instance
            .get_memory(&mut store, exports::MEMORY)
            .ok_or("no memory export")?;
        let func = |name: &str| format!("no {name} export");
        let plugin = Self {
            alloc: instance
                .get_typed_func(&mut store, exports::ALLOC)
                .map_err(|_| func(exports::ALLOC))?,
            enable: instance
                .get_typed_func(&mut store, exports::ENABLE)
                .map_err(|_| func(exports::ENABLE))?,
            disable: instance
                .get_typed_func(&mut store, exports::DISABLE)
                .map_err(|_| func(exports::DISABLE))?,
            tick: instance
                .get_typed_func(&mut store, exports::TICK)
                .map_err(|_| func(exports::TICK))?,
            event: instance
                .get_typed_func(&mut store, exports::EVENT)
                .map_err(|_| func(exports::EVENT))?,
            callback: instance.get_typed_func(&mut store, exports::CALLBACK).ok(),
            store,
            exchange,
            memory,
        };
        Ok((plugin, started))
    }

    /// Starts the plugin with its start record.
    ///
    /// # Errors
    /// A fault, or the plugin's refusal.
    pub fn enable(
        &mut self,
        start: &ac_plugin_abi::StartInfo,
        client: &mut dyn Client,
    ) -> Result<CallFuel, String> {
        let (ptr, len, handed) = self.hand_over(&start.to_bytes(), client)?;
        let enable = self.enable.clone();
        let (result, mut fuel) = self.run(enable, (ptr, len), "enable", FUEL_START, client)?;
        if result != 0 {
            return Err(format!("the plugin refused to start ({result:#x})"));
        }
        fuel.absorb(&handed);
        Ok(fuel)
    }

    /// Stops the plugin.
    ///
    /// # Errors
    /// A fault.
    pub fn disable(&mut self, client: &mut dyn Client) -> Result<CallFuel, String> {
        let disable = self.disable.clone();
        Ok(self.run(disable, (), "disable", FUEL_START, client)?.1)
    }

    /// One tick.
    ///
    /// # Errors
    /// A fault.
    pub fn tick(&mut self, client: &mut dyn Client) -> Result<CallFuel, String> {
        let tick = self.tick.clone();
        Ok(self.run(tick, (), "tick", FUEL_PER_CALL, client)?.1)
    }

    /// One event.
    ///
    /// # Errors
    /// A fault.
    pub fn event(
        &mut self,
        event: &ac_plugin_abi::Event,
        client: &mut dyn Client,
    ) -> Result<CallFuel, String> {
        let (ptr, len, handed) = self.hand_over(&event.to_bytes(), client)?;
        let func = self.event.clone();
        let (_, mut fuel) = self.run(func, (ptr, len), "event", FUEL_PER_CALL, client)?;
        // The buffer is a call of its own on the host, with its own budget;
        // it is counted with the event it carries, the stricter reading.
        fuel.absorb(&handed);
        Ok(fuel)
    }

    /// One callback the plugin registered (a chat interceptor, a loot
    /// classifier); the buffer it is handed in is counted with it.
    ///
    /// # Errors
    /// A fault, or a plugin without the callback export.
    pub fn callback(
        &mut self,
        callback: &ac_plugin_abi::callback::Callback,
        client: &mut dyn Client,
    ) -> Result<CallFuel, String> {
        let func = self
            .callback
            .clone()
            .ok_or_else(|| format!("no {} export", exports::CALLBACK))?;
        let (ptr, len, handed) = self.hand_over(&callback.to_bytes(), client)?;
        let (_, mut fuel) = self.run(func, (ptr, len), "callback", FUEL_PER_CALL, client)?;
        fuel.absorb(&handed);
        Ok(fuel)
    }

    /// Copies `bytes` into a buffer the plugin allocates.
    fn hand_over(
        &mut self,
        bytes: &[u8],
        client: &mut dyn Client,
    ) -> Result<(i32, i32, CallFuel), String> {
        let len = i32::try_from(bytes.len()).map_err(|_| "a record larger than 2 GiB")?;
        let alloc = self.alloc.clone();
        let (ptr, fuel) = self.run(alloc, len, "alloc", FUEL_PER_CALL, client)?;
        let offset = usize::try_from(ptr).map_err(|_| "a negative buffer")?;
        self.memory
            .write(&mut self.store, offset, bytes)
            .map_err(|_| "a buffer outside the plugin's memory".to_owned())?;
        Ok((ptr, len, fuel))
    }

    /// Runs one export to its end, answering its imports, and counts it.
    fn run<P: WasmParams + Sync, R: WasmResults + Sync>(
        &mut self,
        func: TypedFunc<P, R>,
        params: P,
        name: &str,
        budget: u64,
        client: &mut dyn Client,
    ) -> Result<(R, CallFuel), String> {
        self.store
            .set_fuel(MEASURING_FUEL)
            .map_err(|error| error.to_string())?;
        let data = self.store.data_mut();
        data.panic = None;
        data.granted = 0;
        *lock(&self.exchange) = Exchange::default();
        let mut fuel = CallFuel {
            call: name.to_owned(),
            budget,
            ..CallFuel::default()
        };
        let mut traced = 0u64;
        let result = drive(
            func.call_async(&mut self.store, params),
            &self.exchange,
            &mut fuel,
            &mut traced,
            client,
        )?;
        match result {
            Ok(result) => {
                fuel.granted = self.store.data().granted;
                let left = self.store.get_fuel().map_err(|error| error.to_string())?;
                fuel.used = (MEASURING_FUEL + fuel.granted).saturating_sub(left);
                Ok((result, fuel))
            }
            Err(error) => {
                let panic = self.store.data_mut().panic.take();
                Err(format!(
                    "{name}: {}",
                    panic.unwrap_or_else(|| error.to_string())
                ))
            }
        }
    }
}

/// Drives one call to its end as the host does (`ac_plugin::instance::drive`):
/// each time it waits on an import, `client` answers it and the call goes
/// on; each import is counted and profiled into `fuel`.
fn drive<R>(
    call: impl Future<Output = wasmtime::Result<R>>,
    exchange: &Mutex<Exchange>,
    fuel: &mut CallFuel,
    traced: &mut u64,
    client: &mut dyn Client,
) -> Result<wasmtime::Result<R>, String> {
    let mut call = pin!(call);
    let mut context = Context::from_waker(Waker::noop());
    loop {
        if let Poll::Ready(result) = call.as_mut().poll(&mut context) {
            return Ok(result);
        }
        let Some((import, request, left, granted)) = lock(exchange).pending.take() else {
            return Err("a plugin call stopped without a request".to_owned());
        };
        *fuel.imports.entry(import.name()).or_default() += 1;
        let ran = (MEASURING_FUEL + granted).saturating_sub(left);
        fuel.trace
            .push((trace_name(import, &request), ran.saturating_sub(*traced)));
        *traced = ran;
        let reply = client.call(import, &request);
        lock(exchange).reply = Some(reply);
    }
}

/// An import's name in a call's profile: a marker's text for a log line
/// that starts with `@`.
fn trace_name(import: Import, request: &[u8]) -> std::borrow::Cow<'static, str> {
    if import == Import::HostLog
        && let Ok(log) = ac_plugin_abi::types::LogRequest::from_bytes(request)
        && log.message.starts_with('@')
    {
        return log.message.into();
    }
    import.name().into()
}
