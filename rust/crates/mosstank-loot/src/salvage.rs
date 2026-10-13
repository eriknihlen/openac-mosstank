//! Combining salvage bags (`SalvageBagCombinePlanner`): which bags of one
//! material go together, by the profile's workmanship ranges, and how many.

use std::cmp::Ordering;
use std::collections::HashSet;
use std::sync::OnceLock;

use ac_plugin_abi::items::InventoryItem;
use mosstank_dotnet::try_parse_double;
use mosstank_formats::utl::VtankSalvageCombineSettings;
use openac_plugin_compat::regex::{DotNetRegex, RegexOptions};
use openac_plugin_compat::text::casing;

/// The bags to combine in one salvage use.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SalvageBagCombinePlan {
    /// The bags, in the order they go to the tool.
    pub object_ids: Vec<u32>,
    /// Their material.
    pub material_type: u32,
    /// Their names, in the same order.
    pub names: Vec<String>,
}

impl SalvageBagCombinePlan {
    /// The first bag, 0 for none.
    #[must_use]
    pub fn first_object_id(&self) -> u32 {
        self.object_ids.first().copied().unwrap_or(0)
    }

    /// The second bag, 0 for none.
    #[must_use]
    pub fn second_object_id(&self) -> u32 {
        self.object_ids.get(1).copied().unwrap_or(0)
    }

    /// The first bag's name.
    #[must_use]
    pub fn first_name(&self) -> &str {
        self.names.first().map_or("", String::as_str)
    }

    /// The second bag's name.
    #[must_use]
    pub fn second_name(&self) -> &str {
        self.names.get(1).map_or("", String::as_str)
    }
}

fn salvage_bag_name() -> &'static DotNetRegex {
    static PATTERN: OnceLock<DotNetRegex> = OnceLock::new();
    PATTERN.get_or_init(|| {
        DotNetRegex::new(
            r"^Salvage(?:d)?.* \([0-9]{1,2}\)$",
            RegexOptions::IGNORE_CASE,
        )
        .expect("the salvage bag pattern compiles")
    })
}

/// `float.CompareTo`: NaN first, and the two zeros equal.
fn compare_float(left: f32, right: f32) -> Ordering {
    match left.partial_cmp(&right) {
        Some(order) => order,
        None => match (left.is_nan(), right.is_nan()) {
            (true, true) => Ordering::Equal,
            (true, false) => Ordering::Less,
            _ => Ordering::Greater,
        },
    }
}

/// The next bags to combine, or `None`.
#[must_use]
pub fn plan(
    owned: &[InventoryItem],
    abandoned: Option<&HashSet<u32>>,
    settings: Option<&VtankSalvageCombineSettings>,
) -> Option<SalvageBagCombinePlan> {
    let default_settings = VtankSalvageCombineSettings::default();
    let settings = settings.unwrap_or(&default_settings);
    let mut bags: Vec<&InventoryItem> = owned
        .iter()
        .filter(|item| {
            item.material_type != 0
                && salvage_bag_name().is_match(&item.name).unwrap_or(false)
                && abandoned.is_none_or(|set| !set.contains(&item.object_id))
        })
        .collect();
    bags.sort_by(|left, right| {
        left.material_type
            .cmp(&right.material_type)
            .then_with(|| compare_float(left.workmanship, right.workmanship))
            .then_with(|| left.object_id.cmp(&right.object_id))
    });

    let mut start = 0;
    while start < bags.len() {
        let material = bags[start].material_type;
        let end = start
            + bags[start..]
                .iter()
                .take_while(|bag| bag.material_type == material)
                .count();
        let group = &bags[start..end];
        start = end;

        let material_key = i32::try_from(material).ok();
        let combine = material_key
            .and_then(|key| settings.material_combine_strings.get(&key))
            .unwrap_or(&settings.default_combine_string);
        let ranges = parse_combine_string(combine);

        // Bins in order of first appearance, then by range index.
        let mut bins: Vec<(i32, Vec<&InventoryItem>)> = Vec::new();
        for &bag in group {
            let index = range_index(&ranges, f64::from(bag.workmanship));
            match bins.iter_mut().find(|(key, _)| *key == index) {
                Some((_, members)) => members.push(bag),
                None => bins.push((index, vec![bag])),
            }
        }
        bins.sort_by_key(|(key, _)| *key);

        for (_, candidates) in bins {
            if candidates.len() < 2 {
                continue;
            }
            let selected: Vec<&InventoryItem> = if let Some(&target) =
                material_key.and_then(|key| settings.material_value_mode_values.get(&key))
            {
                let total: i64 = candidates.iter().map(|item| i64::from(item.value)).sum();
                if total >= i64::from(target) {
                    candidates
                } else {
                    let pair = find_sub_hundred_pair(&candidates);
                    if pair.is_empty() {
                        continue;
                    }
                    pair
                }
            } else {
                let mut maximum_bags = Vec::new();
                let mut units = 0i32;
                for candidate in candidates {
                    maximum_bags.push(candidate);
                    units = units.wrapping_add(candidate.structure.max(0));
                    if units >= 100 {
                        break;
                    }
                }
                maximum_bags
            };
            return Some(SalvageBagCombinePlan {
                object_ids: selected.iter().map(|item| item.object_id).collect(),
                material_type: material,
                names: selected.iter().map(|item| item.name.clone()).collect(),
            });
        }
    }
    None
}

/// Whether two workmanships fall in the same one of VTank's fixed bands.
#[must_use]
#[allow(clippy::float_cmp)]
pub fn same_vtank_workmanship_band(left: f32, right: f32) -> bool {
    (left < 7.0 && right < 7.0)
        || ((7.0..9.0).contains(&left) && (7.0..9.0).contains(&right))
        || ((9.0..10.0).contains(&left) && (9.0..10.0).contains(&right))
        || (left == 10.0 && right == 10.0)
}

/// Whether two workmanships fall in the same range of a combine string.
#[must_use]
pub fn same_combine_band(left: f32, right: f32, combine_string: &str) -> bool {
    let ranges = parse_combine_string(combine_string);
    range_index(&ranges, f64::from(left)) == range_index(&ranges, f64::from(right))
}

fn find_sub_hundred_pair<'a>(candidates: &[&'a InventoryItem]) -> Vec<&'a InventoryItem> {
    for left in 0..candidates.len().saturating_sub(1) {
        for right in left + 1..candidates.len() {
            if candidates[left].structure.max(0) + candidates[right].structure.max(0) < 100 {
                return vec![candidates[left], candidates[right]];
            }
        }
    }
    Vec::new()
}

/// `Split(separators, RemoveEmptyEntries | TrimEntries)`.
fn split_trimmed<'a>(text: &'a str, separators: &[char]) -> Vec<&'a str> {
    text.split(separators)
        .map(|part| part.trim_matches(casing::is_white_space))
        .filter(|part| !part.is_empty())
        .collect()
}

/// The ranges of a combine string: `"1-5, 6-10"`; a single number is a
/// range of one, and an upper bound that does not read is zero.
fn parse_combine_string(source: &str) -> Vec<(f64, f64)> {
    let mut result = Vec::new();
    for token in split_trimmed(source, &[',', ';']) {
        let bounds = split_trimmed(token, &['-']);
        let Some(minimum) = bounds.first().and_then(|first| try_parse_double(first)) else {
            continue;
        };
        let maximum = if bounds.len() > 1 {
            try_parse_double(bounds[1]).unwrap_or(0.0)
        } else {
            minimum
        };
        result.push((minimum, maximum));
    }
    result
}

fn range_index(ranges: &[(f64, f64)], value: f64) -> i32 {
    for (index, &(minimum, maximum)) in ranges.iter().enumerate() {
        let index = i32::try_from(index).unwrap_or(i32::MAX);
        if minimum > value {
            return index - 1;
        }
        if minimum <= value && maximum >= value {
            return index;
        }
    }
    i32::try_from(ranges.len()).unwrap_or(i32::MAX)
}
