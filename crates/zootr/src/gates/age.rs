use std::collections::HashMap;

use anyhow::Result;
use ounce_formats::Romfs;

use super::{Atom, Expr, creators};
use crate::world::Era;

/// Placements whose creators in this file all directly ask for the same age.
pub(crate) fn creation_ages(romfs: &Romfs, file: &str, gyaml: &str) -> Result<HashMap<u64, Era>> {
    let mut ages = HashMap::new();
    for c in creators(romfs)?
        .into_iter()
        .filter(|c| c.file == file && c.gyaml == gyaml)
    {
        let age = match c.gate {
            Expr::Atom(Atom::Child) => Some(Era::Child),
            Expr::Atom(Atom::Adult) => Some(Era::Adult),
            _ => None,
        };
        ages.entry(c.hash)
            .and_modify(|prior| {
                if *prior != age {
                    *prior = None;
                }
            })
            .or_insert(age);
    }
    Ok(ages
        .into_iter()
        .filter_map(|(hash, age)| Some((hash, age?)))
        .collect())
}
