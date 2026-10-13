//! Runs the built `mosstank.wasm` the way the openAC plugin host runs a
//! plugin, and counts the fuel (WebAssembly instructions) every call uses.
//!
//! The host (openac-rust `crates/ac-plugin/src/instance.rs`) gives each call
//! a budget: [`FUEL_START`] for instantiation, `enable` and `disable`,
//! [`FUEL_PER_CALL`] for a tick, an event, a callback and the buffer the
//! client hands over before an event. A page of a large table
//! (`spells_page`) brings the fuel to decode it, up to
//! `FUEL_PAGED_GRANT_MAX` per call. Running out is a fault that unloads the
//! plugin.
//!
//! This runner is that loop with the budget lifted: the same engine
//! configuration (fuel metering, lazy translation), the same import stubs
//! (suspend, the caller answers, resume), the same grant, so a call uses
//! the same fuel here as on the host. What differs is who answers the
//! imports: a [`Client`] of the caller's (the plugin's test client) in
//! place of the session. [`CallFuel::fits`] says whether the call would
//! have finished on the host.

use std::collections::BTreeMap;
use std::fmt;

use ac_plugin_abi::{Import, MODULE, Wire, exports};
use wasmi::errors::HostError;
use wasmi::{
    Caller, CompilationMode, Config, Engine, Extern, Linker, Memory, Module, Store, StoreLimits,
    StoreLimitsBuilder, TypedFunc, TypedResumableCall, Val, WasmParams, WasmResults,
};

/// The host's fuel for one tick, event or command (openac-rust
/// `ac_plugin::FUEL_PER_CALL`).
pub const FUEL_PER_CALL: u64 = 100_000_000;

/// The host's fuel for starting (instantiation and `enable`) and stopping
/// (openac-rust `ac_plugin::FUEL_START`).
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
    /// the one before (the call's profile).
    pub trace: Vec<(&'static str, u64)>,
}

impl CallFuel {
    /// What the host would have allowed: the budget and the grants.
    #[must_use]
    pub fn allowed(&self) -> u64 {
        self.budget + self.granted
    }

    /// Whether the call finishes on the host.
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

/// The store's data: what an import stub leaves for the caller.
#[derive(Debug)]
struct StoreData {
    limits: StoreLimits,
    pending: Option<(Import, Vec<u8>)>,
    reply: Vec<u8>,
    panic: Option<String>,
}

/// The stub's signal that a request waits for the caller.
#[derive(Debug)]
struct Suspend;

impl fmt::Display for Suspend {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a plugin call waits for the client")
    }
}

impl HostError for Suspend {}

fn memory_of(caller: &Caller<'_, StoreData>) -> Result<Memory, wasmi::Error> {
    caller
        .get_export(exports::MEMORY)
        .and_then(Extern::into_memory)
        .ok_or_else(|| wasmi::Error::new("the plugin has no memory"))
}

fn read_request(
    caller: &Caller<'_, StoreData>,
    ptr: i32,
    len: i32,
) -> Result<Vec<u8>, wasmi::Error> {
    let (Ok(offset), Ok(len)) = (usize::try_from(ptr), usize::try_from(len)) else {
        return Err(wasmi::Error::new("a negative pointer or length"));
    };
    let mut bytes = vec![0u8; len];
    memory_of(caller)?
        .read(caller, offset, &mut bytes)
        .map_err(|_| wasmi::Error::new("a request outside the plugin's memory"))?;
    Ok(bytes)
}

/// Links one import as the host does.
fn link(linker: &mut Linker<StoreData>, import: Import) -> Result<(), wasmi::Error> {
    let name = import.name();
    match import {
        Import::AbiReplyRead => linker.func_wrap(
            MODULE,
            name,
            |mut caller: Caller<'_, StoreData>, ptr: i32, len: i32| -> Result<i32, wasmi::Error> {
                let reply = std::mem::take(&mut caller.data_mut().reply);
                let (Ok(offset), Ok(len)) = (usize::try_from(ptr), usize::try_from(len)) else {
                    return Err(wasmi::Error::new("a reply buffer with a negative length"));
                };
                let take = len.min(reply.len());
                memory_of(&caller)?
                    .write(&mut caller, offset, &reply[..take])
                    .map_err(|_| wasmi::Error::new("a reply buffer outside the plugin's memory"))?;
                Ok(0)
            },
        )?,
        Import::AbiPanic => linker.func_wrap(
            MODULE,
            name,
            |mut caller: Caller<'_, StoreData>, ptr: i32, len: i32| -> Result<i32, wasmi::Error> {
                let bytes = read_request(&caller, ptr, len)?;
                caller.data_mut().panic = Some(String::from_utf8_lossy(&bytes).into_owned());
                Ok(0)
            },
        )?,
        _ => linker.func_wrap(
            MODULE,
            name,
            move |mut caller: Caller<'_, StoreData>,
                  ptr: i32,
                  len: i32|
                  -> Result<i32, wasmi::Error> {
                let request = read_request(&caller, ptr, len)?;
                caller.data_mut().pending = Some((import, request));
                Err(wasmi::Error::host(Suspend))
            },
        )?,
    };
    Ok(())
}

/// The built plugin, instantiated.
#[allow(missing_debug_implementations)]
pub struct Plugin {
    store: Store<StoreData>,
    memory: Memory,
    alloc: TypedFunc<i32, i32>,
    enable: TypedFunc<(i32, i32), i64>,
    disable: TypedFunc<(), ()>,
    tick: TypedFunc<(), ()>,
    event: TypedFunc<(i32, i32), ()>,
}

impl Plugin {
    /// Instantiates `wasm` with every import it asks for linked (the host
    /// links only the permitted ones; MossTank's manifest permits all it
    /// uses). The fuel instantiation used is the first measured call.
    ///
    /// # Errors
    /// The module does not load or link, or its start traps.
    pub fn load(wasm: &[u8], memory_mib: u32) -> Result<(Self, CallFuel), String> {
        let mut config = Config::default();
        config
            .consume_fuel(true)
            .compilation_mode(CompilationMode::LazyTranslation);
        let engine = Engine::new(&config);
        let module = Module::new(&engine, wasm).map_err(|error| error.to_string())?;
        let mut linker = Linker::<StoreData>::new(&engine);
        for import in module.imports() {
            let import = Import::from_name(import.name())
                .ok_or_else(|| format!("an unknown import {}", import.name()))?;
            link(&mut linker, import).map_err(|error| error.to_string())?;
        }
        let bytes = usize::try_from(memory_mib).unwrap_or(usize::MAX) << 20;
        let mut store = Store::new(
            &engine,
            StoreData {
                limits: StoreLimitsBuilder::new()
                    .memory_size(bytes)
                    .trap_on_grow_failure(true)
                    .build(),
                pending: None,
                reply: Vec::new(),
                panic: None,
            },
        );
        store.limiter(|data| &mut data.limits);
        store
            .set_fuel(MEASURING_FUEL)
            .map_err(|error| error.to_string())?;
        let instance = linker
            .instantiate_and_start(&mut store, &module)
            .map_err(|error| error.to_string())?;
        let left = store.get_fuel().map_err(|error| error.to_string())?;
        let started = CallFuel {
            call: "instantiate".to_owned(),
            budget: FUEL_START,
            used: MEASURING_FUEL - left,
            ..CallFuel::default()
        };
        let memory = instance
            .get_memory(&store, exports::MEMORY)
            .ok_or("no memory export")?;
        let func = |name: &str| format!("no {name} export");
        let plugin = Self {
            alloc: instance
                .get_typed_func(&store, exports::ALLOC)
                .map_err(|_| func(exports::ALLOC))?,
            enable: instance
                .get_typed_func(&store, exports::ENABLE)
                .map_err(|_| func(exports::ENABLE))?,
            disable: instance
                .get_typed_func(&store, exports::DISABLE)
                .map_err(|_| func(exports::DISABLE))?,
            tick: instance
                .get_typed_func(&store, exports::TICK)
                .map_err(|_| func(exports::TICK))?,
            event: instance
                .get_typed_func(&store, exports::EVENT)
                .map_err(|_| func(exports::EVENT))?,
            store,
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
        let (result, mut fuel) = self.run(self.enable, (ptr, len), "enable", FUEL_START, client)?;
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
        Ok(self.run(self.disable, (), "disable", FUEL_START, client)?.1)
    }

    /// One tick.
    ///
    /// # Errors
    /// A fault.
    pub fn tick(&mut self, client: &mut dyn Client) -> Result<CallFuel, String> {
        Ok(self.run(self.tick, (), "tick", FUEL_PER_CALL, client)?.1)
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
        let (_, mut fuel) = self.run(self.event, (ptr, len), "event", FUEL_PER_CALL, client)?;
        // The buffer is a call of its own on the host, with its own budget;
        // it is counted with the event it carries, the stricter reading.
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
        let (ptr, fuel) = self.run(self.alloc, len, "alloc", FUEL_PER_CALL, client)?;
        let offset = usize::try_from(ptr).map_err(|_| "a negative buffer")?;
        self.memory
            .write(&mut self.store, offset, bytes)
            .map_err(|_| "a buffer outside the plugin's memory".to_owned())?;
        Ok((ptr, len, fuel))
    }

    /// Runs one export to its end, answering its imports, and counts it.
    fn run<P: WasmParams, R: WasmResults>(
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
        self.store.data_mut().panic = None;
        let mut fuel = CallFuel {
            call: name.to_owned(),
            budget,
            ..CallFuel::default()
        };
        let mut traced = 0u64;
        let mut call = func.call_resumable(&mut self.store, params);
        loop {
            match call {
                Ok(TypedResumableCall::Finished(result)) => {
                    let left = self.store.get_fuel().map_err(|error| error.to_string())?;
                    fuel.used = MEASURING_FUEL + fuel.granted - left;
                    return Ok((result, fuel));
                }
                Ok(TypedResumableCall::OutOfFuel(_)) => {
                    return Err(format!("{name}: past the measuring fuel"));
                }
                Ok(TypedResumableCall::HostTrap(invocation)) => {
                    if invocation.host_error().downcast_ref::<Suspend>().is_none() {
                        return Err(invocation.host_error().to_string());
                    }
                    let Some((import, request)) = self.store.data_mut().pending.take() else {
                        return Err("a suspended call with no request".to_owned());
                    };
                    *fuel.imports.entry(import.name()).or_default() += 1;
                    let left = self.store.get_fuel().map_err(|error| error.to_string())?;
                    let ran = MEASURING_FUEL + fuel.granted - left;
                    fuel.trace.push((import.name(), ran - traced));
                    traced = ran;
                    let reply = client.call(import, &request);
                    let len = i32::try_from(reply.len()).map_err(|_| "a reply over 2 GiB")?;
                    // A page of a large table brings the fuel to decode it,
                    // up to a bound per call, as on the host.
                    if let Some(records) = ac_plugin_abi::magic::paged_records(import, &reply) {
                        let grant = (records as u64)
                            .saturating_mul(ac_plugin_abi::magic::FUEL_PER_PAGED_RECORD)
                            .min(ac_plugin_abi::magic::FUEL_PAGED_GRANT_MAX - fuel.granted);
                        fuel.granted += grant;
                        let left = self.store.get_fuel().map_err(|error| error.to_string())?;
                        self.store
                            .set_fuel(left.saturating_add(grant))
                            .map_err(|error| error.to_string())?;
                    }
                    self.store.data_mut().reply = reply;
                    call = invocation.resume(&mut self.store, &[Val::I32(len)]);
                }
                Err(error) => {
                    let panic = self.store.data_mut().panic.take();
                    return Err(format!(
                        "{name}: {}",
                        panic.unwrap_or_else(|| error.to_string())
                    ));
                }
            }
        }
    }
}
