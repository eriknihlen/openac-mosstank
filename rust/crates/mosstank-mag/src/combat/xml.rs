//! The combat stats files, in Mag-Tools' format (`CombatTrackerExporter`,
//! `CombatTrackerImporter`), under the plugin's own data:
//!
//! - the persistent stats: `Mag-Tools/<world>/<name>.CombatTracker.xml`;
//! - an export: `Mag-Tools/<world>/<name>.CombatTracker.<yyyy-MM-dd HH-mm>.xml`
//!   (local time; a second export in the same minute replaces the first).
//!
//! ```xml
//! <CombatTracker>
//!   <CombatInfos>
//!     <CombatInfo SourceName="Acdream" TargetName="Drudge Skulker" KillingBlows="1">
//!       <MeleeMissle>
//!         <Slash TotalAttacks="3" FailedAttacks="1" TotalNormalDamage="50" MaxNormalDamage="25" />
//!       </MeleeMissle>
//!     </CombatInfo>
//!   </CombatInfos>
//!   <AetheriaInfos>
//!     <AetheriaInfo SourceName="Acdream" TargetName="Acdream" TotalSurges="2" />
//!   </AetheriaInfos>
//!   <CloakInfos />
//! </CombatTracker>
//! ```
//!
//! Written as .NET's `XmlDocument.Save` writes it: no declaration, UTF-8
//! without a byte-order mark, two-space indents, CRLF line breaks, no
//! final line break, `" />"` for an empty element, `&amp; &lt; &gt; &quot;`
//! escaped in attribute values; an attribute only when it is not zero
//! (the names always), a section only when it has infos.

use openac_plugin_compat::datetime::DateTime;

use super::CombatTracker;
use super::info::{
    AttackType, CombatInfo, DamageByAttackType, DamageByElement, DamageElement, SurgeInfo,
};
use crate::host::MagHost;
use crate::inventory_log::file::key_part;

/// The persistent stats' key.
#[must_use]
pub fn persistent_key(world: &str, character: &str) -> String {
    format!(
        "Mag-Tools/{}/{}.CombatTracker.xml",
        key_part(world),
        key_part(character)
    )
}

/// An export's key, named for the local minute `local`
/// (`DateTime.Now.ToString("yyyy-MM-dd HH-mm")`).
#[must_use]
pub fn export_key(world: &str, character: &str, local: DateTime) -> String {
    let stamp = local.format("yyyy-MM-dd HH-mm").unwrap_or_default();
    format!(
        "Mag-Tools/{}/{}.CombatTracker.{stamp}.xml",
        key_part(world),
        key_part(character)
    )
}

const NEW_LINE: &str = "\r\n";

/// An attribute value escaped as `XmlTextWriter` escapes one.
fn escape_into(out: &mut String, value: &str) {
    for c in value.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            c => out.push(c),
        }
    }
}

/// An element being written: its start tag is open until a child or its
/// end comes.
struct Writer {
    out: String,
    /// The open elements' names, and whether each has a child yet.
    open: Vec<(&'static str, bool)>,
}

impl Writer {
    fn new() -> Self {
        Self {
            out: String::new(),
            open: Vec::new(),
        }
    }

    fn start(&mut self, name: &'static str) {
        if let Some((_, has_child)) = self.open.last_mut()
            && !*has_child
        {
            self.out.push('>');
            *has_child = true;
        }
        if !self.open.is_empty() {
            self.out.push_str(NEW_LINE);
            for _ in 0..self.open.len() {
                self.out.push_str("  ");
            }
        }
        self.out.push('<');
        self.out.push_str(name);
        self.open.push((name, false));
    }

    fn text_attribute(&mut self, name: &str, value: &str) {
        self.out.push(' ');
        self.out.push_str(name);
        self.out.push_str("=\"");
        escape_into(&mut self.out, value);
        self.out.push('"');
    }

    /// An attribute written only when it is not zero.
    fn number_attribute(&mut self, name: &str, value: i32) {
        if value != 0 {
            self.out.push(' ');
            self.out.push_str(name);
            self.out.push_str("=\"");
            self.out.push_str(&value.to_string());
            self.out.push('"');
        }
    }

    fn end(&mut self) {
        let Some((name, has_child)) = self.open.pop() else {
            return;
        };
        if has_child {
            self.out.push_str(NEW_LINE);
            for _ in 0..self.open.len() {
                self.out.push_str("  ");
            }
            self.out.push_str("</");
            self.out.push_str(name);
            self.out.push('>');
        } else {
            self.out.push_str(" />");
        }
    }
}

fn write_surges(
    writer: &mut Writer,
    section: &'static str,
    element: &'static str,
    infos: &[SurgeInfo],
) {
    if infos.is_empty() {
        return;
    }
    writer.start(section);
    for info in infos {
        writer.start(element);
        writer.text_attribute("SourceName", &info.source);
        writer.text_attribute("TargetName", &info.target);
        writer.number_attribute("TotalSurges", info.total_surges);
        writer.end();
    }
    writer.end();
}

/// The tracker's stats file (`CombatTrackerExporter.Export`).
#[must_use]
pub fn export(tracker: &CombatTracker) -> String {
    let mut writer = Writer::new();
    writer.start("CombatTracker");
    if !tracker.combat_infos().is_empty() {
        writer.start("CombatInfos");
        for info in tracker.combat_infos() {
            writer.start("CombatInfo");
            writer.text_attribute("SourceName", &info.source);
            writer.text_attribute("TargetName", &info.target);
            writer.number_attribute("KillingBlows", info.killing_blows);
            for (attack_type, by_type) in info.by_attack_type.iter() {
                writer.start(attack_type.name());
                for (element, bucket) in by_type.by_element.iter() {
                    writer.start(element.name());
                    // TotalAttacks is always written.
                    writer.text_attribute("TotalAttacks", &bucket.total_attacks.to_string());
                    writer.number_attribute("FailedAttacks", bucket.failed_attacks);
                    writer.number_attribute("Crits", bucket.crits);
                    writer.number_attribute("TotalNormalDamage", bucket.total_normal_damage);
                    writer.number_attribute("MaxNormalDamage", bucket.max_normal_damage);
                    writer.number_attribute("TotalCritDamage", bucket.total_crit_damage);
                    writer.number_attribute("MaxCritDamage", bucket.max_crit_damage);
                    writer.end();
                }
                writer.end();
            }
            writer.end();
        }
        writer.end();
    }
    write_surges(
        &mut writer,
        "AetheriaInfos",
        "AetheriaInfo",
        tracker.aetheria_infos(),
    );
    write_surges(
        &mut writer,
        "CloakInfos",
        "CloakInfo",
        tracker.cloak_infos(),
    );
    writer.end();
    writer.out
}

/// Why a stats file was not read in (whole).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImportError {
    /// Not XML (nothing read in).
    Xml(String),
    /// An info without its `SourceName` or `TargetName` (the infos before
    /// it are read in).
    MissingName(&'static str),
    /// A child of an info that is not an attack type (the infos before it
    /// are read in).
    UnknownAttackType(String),
    /// An element node that is not a damage element.
    UnknownElement(String),
    /// The same attack type or element twice in one info.
    Duplicate(String),
}

impl std::fmt::Display for ImportError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Xml(error) => write!(f, "not XML: {error}"),
            Self::MissingName(name) => write!(f, "an info has no {name}"),
            Self::UnknownAttackType(name) => write!(f, "'{name}' is not an attack type"),
            Self::UnknownElement(name) => write!(f, "'{name}' is not a damage element"),
            Self::Duplicate(name) => write!(f, "'{name}' is there twice"),
        }
    }
}

/// What reading a file in did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Imported {
    /// Read in: the tracker holds the file's stats.
    Read,
    /// No `CombatTracker` root: the tracker is as it was.
    NotStats,
}

/// `int.TryParse` of an attribute, 0 when missing or not a number.
fn int_attribute(node: roxmltree::Node<'_, '_>, name: &str) -> i32 {
    node.attribute(name)
        .and_then(mosstank_dotnet::try_parse_int)
        .unwrap_or(0)
}

fn has_attributes(node: roxmltree::Node<'_, '_>) -> bool {
    node.is_element() && node.attributes().len() > 0
}

/// The source and target names of an info node.
fn names(node: roxmltree::Node<'_, '_>) -> Result<(String, String), ImportError> {
    let source = node
        .attribute("SourceName")
        .ok_or(ImportError::MissingName("SourceName"))?;
    let target = node
        .attribute("TargetName")
        .ok_or(ImportError::MissingName("TargetName"))?;
    Ok((source.to_owned(), target.to_owned()))
}

/// A node that `XmlDocument` (whitespace not kept) has as a child.
fn is_child_node(node: roxmltree::Node<'_, '_>) -> bool {
    match node.node_type() {
        roxmltree::NodeType::Text => !node.text().unwrap_or_default().trim().is_empty(),
        _ => true,
    }
}

/// The name `XmlNode.Name` gives a node that is not an element.
fn node_name(node: roxmltree::Node<'_, '_>) -> String {
    match node.node_type() {
        roxmltree::NodeType::Element => node.tag_name().name().to_owned(),
        roxmltree::NodeType::Comment => "#comment".to_owned(),
        roxmltree::NodeType::PI => node
            .pi()
            .map_or_else(String::new, |pi| pi.target.to_owned()),
        _ => "#text".to_owned(),
    }
}

fn combat_info(node: roxmltree::Node<'_, '_>) -> Result<CombatInfo, ImportError> {
    let (source, target) = names(node)?;
    let mut info = CombatInfo::new(&source, &target);
    info.killing_blows = int_attribute(node, "KillingBlows");
    for type_node in node.children().filter(|n| is_child_node(*n)) {
        let name = node_name(type_node);
        let attack_type = (type_node.is_element())
            .then(|| AttackType::from_name(&name))
            .flatten()
            .ok_or_else(|| ImportError::UnknownAttackType(name.clone()))?;
        let mut by_type = DamageByAttackType::default();
        for element_node in type_node.children().filter(|n| has_attributes(*n)) {
            let element_name = element_node.tag_name().name();
            let bucket = DamageByElement {
                total_attacks: int_attribute(element_node, "TotalAttacks"),
                failed_attacks: int_attribute(element_node, "FailedAttacks"),
                crits: int_attribute(element_node, "Crits"),
                total_normal_damage: int_attribute(element_node, "TotalNormalDamage"),
                max_normal_damage: int_attribute(element_node, "MaxNormalDamage"),
                total_crit_damage: int_attribute(element_node, "TotalCritDamage"),
                max_crit_damage: int_attribute(element_node, "MaxCritDamage"),
            };
            let element = DamageElement::from_name(element_name)
                .ok_or_else(|| ImportError::UnknownElement(element_name.to_owned()))?;
            if !by_type.by_element.add(element, bucket) {
                return Err(ImportError::Duplicate(element_name.to_owned()));
            }
        }
        if !info.by_attack_type.add(attack_type, by_type) {
            return Err(ImportError::Duplicate(name));
        }
    }
    Ok(info)
}

fn surge_info(node: roxmltree::Node<'_, '_>) -> Result<SurgeInfo, ImportError> {
    let (source, target) = names(node)?;
    let mut info = SurgeInfo::new(&source, &target);
    info.total_surges = int_attribute(node, "TotalSurges");
    Ok(info)
}

/// The section an info belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Section {
    Combat,
    Aetheria,
    Cloak,
}

impl Section {
    fn of(name: &str) -> Option<Self> {
        match name {
            "CombatInfos" => Some(Self::Combat),
            "AetheriaInfos" => Some(Self::Aetheria),
            "CloakInfos" => Some(Self::Cloak),
            _ => None,
        }
    }
}

fn xml_error(at: usize, what: &str) -> ImportError {
    ImportError::Xml(format!("{what} at byte {at}"))
}

/// Where `needle` is at or after `from`.
fn find_from(text: &str, from: usize, needle: &str) -> Option<usize> {
    text.get(from..)?.find(needle).map(|at| from + at)
}

/// The end of the start tag that begins at `start` (after its `>`), and
/// whether it closes itself.
fn start_tag_end(bytes: &[u8], start: usize) -> Result<(usize, bool), ImportError> {
    let mut at = start + 1;
    let mut quote = None;
    while at < bytes.len() {
        let b = bytes[at];
        match quote {
            Some(q) if b == q => quote = None,
            Some(_) => {}
            None if b == b'"' || b == b'\'' => quote = Some(b),
            None if b == b'<' => return Err(xml_error(at, "a '<' inside a tag")),
            None if b == b'>' => return Ok((at + 1, bytes[at - 1] == b'/')),
            None => {}
        }
        at += 1;
    }
    Err(xml_error(start, "an unclosed tag"))
}

/// Where the name of the tag whose text starts at `start` ends.
fn tag_name_end(bytes: &[u8], start: usize, end: usize) -> usize {
    bytes[start..end]
        .iter()
        .position(|&b| b.is_ascii_whitespace() || b == b'/' || b == b'>')
        .map_or(end, |i| start + i)
}

/// A file's structure, found a part at a time: the root's name, and where
/// each child element of the first `CombatInfos`, `AetheriaInfos` and
/// `CloakInfos` is (an info each). The tags are checked to nest; what is
/// inside an info is checked when it is read.
#[derive(Debug, Default)]
struct Scanner {
    at: usize,
    /// The open elements' names, as places in the text.
    stack: Vec<std::ops::Range<usize>>,
    root: Option<String>,
    seen_sections: Vec<Section>,
    section: Option<Section>,
    info_start: usize,
    infos: Vec<(Section, std::ops::Range<usize>)>,
}

impl Scanner {
    /// The infos of the section an element closing now ends.
    fn closed(&mut self, start: usize, end: usize) {
        match self.stack.len() {
            2 => {
                if let Some(section) = self.section {
                    self.infos.push((section, start..end));
                }
            }
            1 => self.section = None,
            _ => {}
        }
    }

    /// Scans on until about `budget` bytes further; true at the end.
    fn scan_some(&mut self, text: &str, budget: usize) -> Result<bool, ImportError> {
        let bytes = text.as_bytes();
        let stop = self.at.saturating_add(budget);
        while self.at < stop {
            let Some(open) = bytes
                .get(self.at..)
                .and_then(|rest| rest.iter().position(|&b| b == b'<'))
                .map(|i| self.at + i)
            else {
                return self.end(text).map(|()| true);
            };
            let rest = &text[open..];
            if rest.starts_with("<!--") {
                let end = find_from(text, open + 4, "-->")
                    .ok_or_else(|| xml_error(open, "an unclosed comment"))?;
                self.at = end + 3;
            } else if rest.starts_with("<![CDATA[") {
                let end = find_from(text, open + 9, "]]>")
                    .ok_or_else(|| xml_error(open, "an unclosed CDATA section"))?;
                self.at = end + 3;
            } else if rest.starts_with("<?") {
                let end = find_from(text, open + 2, "?>")
                    .ok_or_else(|| xml_error(open, "an unclosed instruction"))?;
                self.at = end + 2;
            } else if rest.starts_with("<!") {
                // A document type, with its internal subset.
                let mut depth = 0usize;
                let mut end = None;
                for (i, b) in rest.bytes().enumerate().skip(2) {
                    match b {
                        b'[' => depth += 1,
                        b']' => depth = depth.saturating_sub(1),
                        b'>' if depth == 0 => {
                            end = Some(open + i + 1);
                            break;
                        }
                        _ => {}
                    }
                }
                self.at = end.ok_or_else(|| xml_error(open, "an unclosed declaration"))?;
            } else if rest.starts_with("</") {
                let end = bytes[open..]
                    .iter()
                    .position(|&b| b == b'>')
                    .map(|i| open + i)
                    .ok_or_else(|| xml_error(open, "an unclosed end tag"))?;
                let name = text[open + 2..end].trim_end();
                let matches = self
                    .stack
                    .pop()
                    .is_some_and(|open_name| &text[open_name] == name);
                if !matches {
                    return Err(xml_error(open, "an end tag that closes nothing open"));
                }
                self.at = end + 1;
                self.closed(self.info_start, self.at);
            } else {
                let (end, closes_itself) = start_tag_end(bytes, open)?;
                let name_range = open + 1..tag_name_end(bytes, open + 1, end);
                let name = &text[name_range.clone()];
                if name.is_empty() {
                    return Err(xml_error(open, "a tag without a name"));
                }
                match self.stack.len() {
                    0 if self.root.is_some() => {
                        return Err(xml_error(open, "a second root element"));
                    }
                    0 => self.root = Some(name.to_owned()),
                    1 => {
                        // Only the first section of each name is read.
                        self.section =
                            Section::of(name).filter(|s| !self.seen_sections.contains(s));
                        if let Some(s) = self.section {
                            self.seen_sections.push(s);
                        }
                    }
                    2 => self.info_start = open,
                    _ => {}
                }
                self.at = end;
                if closes_itself {
                    self.closed(open, end);
                } else {
                    self.stack.push(name_range);
                }
            }
        }
        Ok(false)
    }

    fn end(&self, text: &str) -> Result<(), ImportError> {
        if !self.stack.is_empty() {
            return Err(xml_error(text.len(), "unclosed elements"));
        }
        if self.root.is_none() {
            return Err(xml_error(0, "no root element"));
        }
        Ok(())
    }
}

/// The infos read so far.
#[derive(Debug, Default)]
struct ReadInfos {
    combat: Vec<CombatInfo>,
    aetheria: Vec<SurgeInfo>,
    cloak: Vec<SurgeInfo>,
}

/// How far a [`Reader`] is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Progress {
    /// More to read.
    More,
    /// Every info read: [`Reader::finish`] next.
    Done,
    /// No `CombatTracker` root: nothing to read in.
    NotStats,
}

/// A stats file read a part at a time ([`Reader::read_some`]): what
/// `CombatTrackerImporter.Import` does, its structure found first, then
/// its infos read aside until the end, so a file that turns out not to be
/// XML leaves the tracker as it was (as `XmlDocument.Load` failing did).
#[derive(Debug)]
pub struct Reader {
    text: String,
    scanner: Scanner,
    scanned: bool,
    next: usize,
    read: ReadInfos,
    /// The first node not of the format: no info is read after it (the
    /// rest is still checked to be XML).
    format_error: Option<ImportError>,
}

impl Reader {
    /// A file to read.
    #[must_use]
    pub fn new(text: String) -> Self {
        let start = if text.starts_with('\u{feff}') { 3 } else { 0 };
        Self {
            text,
            scanner: Scanner {
                at: start,
                ..Scanner::default()
            },
            scanned: false,
            next: 0,
            read: ReadInfos::default(),
            format_error: None,
        }
    }

    /// Reads on about `budget_bytes` of the file (its structure, then its
    /// infos).
    ///
    /// # Errors
    /// The file is not XML.
    pub fn read_some(&mut self, budget_bytes: usize) -> Result<Progress, ImportError> {
        if !self.scanned {
            if !self.scanner.scan_some(&self.text, budget_bytes)? {
                return Ok(Progress::More);
            }
            self.scanned = true;
            if self.scanner.root.as_deref() != Some("CombatTracker") {
                return Ok(Progress::NotStats);
            }
            return Ok(if self.scanner.infos.is_empty() {
                Progress::Done
            } else {
                Progress::More
            });
        }
        let mut read = 0;
        while let Some((section, range)) = self.scanner.infos.get(self.next).cloned() {
            if read >= budget_bytes {
                return Ok(Progress::More);
            }
            self.next += 1;
            read += range.len();
            let document = roxmltree::Document::parse(&self.text[range])
                .map_err(|error| ImportError::Xml(error.to_string()))?;
            let node = document.root_element();
            if self.format_error.is_some() || !has_attributes(node) {
                continue;
            }
            let info = match section {
                Section::Combat => combat_info(node).map(|info| self.read.combat.push(info)),
                Section::Aetheria => surge_info(node).map(|info| self.read.aetheria.push(info)),
                Section::Cloak => surge_info(node).map(|info| self.read.cloak.push(info)),
            };
            if let Err(error) = info {
                self.format_error = Some(error);
            }
        }
        Ok(Progress::Done)
    }

    /// The tracker's lists replaced by what was read.
    ///
    /// # Errors
    /// A node not of the format: the tracker keeps the infos before it.
    pub fn finish(self, tracker: &mut CombatTracker) -> Result<Imported, ImportError> {
        tracker.clear_lists();
        for info in self.read.combat {
            tracker.push_combat_info(info);
        }
        for info in self.read.aetheria {
            tracker.push_aetheria_info(info);
        }
        for info in self.read.cloak {
            tracker.push_cloak_info(info);
        }
        match self.format_error {
            Some(error) => Err(error),
            None => Ok(Imported::Read),
        }
    }
}

/// Reads a stats file into `tracker` at once (`CombatTrackerImporter.Import`):
/// the tracker's lists are emptied, then filled from the file. A node not
/// of the format ends it, the tracker keeping the infos before it.
///
/// # Errors
/// The text is not XML (the tracker is untouched), or a node is not one
/// of the format's.
pub fn import(text: &str, tracker: &mut CombatTracker) -> Result<Imported, ImportError> {
    let mut reader = Reader::new(text.to_owned());
    loop {
        match reader.read_some(usize::MAX)? {
            Progress::More => {}
            Progress::Done => return reader.finish(tracker),
            Progress::NotStats => return Ok(Imported::NotStats),
        }
    }
}

/// The file read a tick: its structure, then its infos.
pub const IMPORT_BYTES_PER_TICK: usize = 96 * 1024;

/// `CombatTracker.ImportStats` a step a tick: the file read, then
/// [`IMPORT_BYTES_PER_TICK`] of it read in a tick. The pages hear
/// [`super::TrackerUpdate::Loaded`] when it is done (also with no file, or
/// one that is not stats), not after an error.
#[derive(Debug)]
pub struct StagedImport {
    key: String,
    reader: Option<Box<Reader>>,
}

impl StagedImport {
    /// An import of the file at `key`.
    #[must_use]
    pub fn new(key: String) -> Self {
        Self { key, reader: None }
    }

    /// The file being read in.
    #[must_use]
    pub fn key(&self) -> &str {
        &self.key
    }

    /// One step; true when the import is done.
    pub fn step(&mut self, host: &mut dyn MagHost, tracker: &mut CombatTracker) -> bool {
        let Some(reader) = &mut self.reader else {
            match host.storage().read_text(&self.key) {
                Ok(Some(text)) => {
                    self.reader = Some(Box::new(Reader::new(text)));
                    return false;
                }
                // No file: the tracker as it was.
                Ok(None) => tracker.note_loaded(),
                Err(error) => super::import_failed(host, &self.key, &ImportError::Xml(error)),
            }
            return true;
        };
        match reader.read_some(IMPORT_BYTES_PER_TICK) {
            Ok(Progress::More) => return false,
            Ok(Progress::NotStats) => tracker.note_loaded(),
            Ok(Progress::Done) => {
                if let Some(reader) = self.reader.take() {
                    match reader.finish(tracker) {
                        Ok(_) => tracker.note_loaded(),
                        Err(error) => super::import_failed(host, &self.key, &error),
                    }
                }
            }
            Err(error) => super::import_failed(host, &self.key, &error),
        }
        self.reader = None;
        true
    }
}
