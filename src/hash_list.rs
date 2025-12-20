// Copyright (c) 2016 est31 <MTest31@outlook.com>
// and contributors. All rights reserved.
// Licensed under MIT license, or Apache 2 license,
// at your option. Please see the LICENSE file
// attached to this source distribution for details.

/*!
Hash list module
*/

use crate::Sha256Hash;
use crate::TaError;
use std::collections::HashMap;
use std::{
    fs::File,
    io::{BufRead, BufReader, BufWriter, Write},
};

pub struct HashList {
    name_to_hash_map: HashMap<String, Sha256Hash>,
}

impl HashList {
    /// Load hash list from a file
    pub fn from_file(path: &str) -> Result<Self, TaError> {
        let rdr = File::open(path)?;
        let mut brdr = BufReader::new(rdr);
        Self::from_reader(&mut brdr)
    }

    /// Load hash list from a reader
    pub fn from_reader<T: BufRead>(brdr: &mut T) -> Result<Self, TaError> {
        let mut name_to_hash_map = HashMap::new();
        for oline in brdr.lines() {
            let line = oline?;
            if line.starts_with('#') {
                continue;
            }
            let mut spi = line.split(' ');
            let Some(hash_str) = spi.next() else {
                continue;
            };
            let hash = Sha256Hash::from_hex(hash_str).map_err(|()| TaError::BadHashFormat)?;
            let Some(name) = spi.next() else {
                continue;
            };
            name_to_hash_map.insert(name.to_owned(), hash);
        }
        Ok(Self { name_to_hash_map })
    }

    /// Save hash list to a file
    pub fn to_file(&self, path: &str) -> Result<(), TaError> {
        let wrt = File::create(path)?;
        let mut bwrtr = BufWriter::new(wrt);
        self.to_writer(&mut bwrtr)
    }

    /// Write hash list to a writer
    pub fn to_writer<W: Write>(&self, bwrtr: &mut BufWriter<W>) -> Result<(), TaError> {
        for (name, hash) in &self.name_to_hash_map {
            bwrtr.write_all(format!("{} {}\n", hash.to_hex(), name).as_bytes())?;
        }
        Ok(())
    }

    /// Create a new empty hash list
    #[must_use]
    pub fn new() -> Self {
        Self { name_to_hash_map: HashMap::new() }
    }

    /// Get the hash for a given filename
    #[must_use]
    pub fn get_hash(&self, filename: &str) -> Option<&Sha256Hash> {
        self.name_to_hash_map.get(filename)
    }

    /// Add or update an entry in the hash list
    pub fn add_entry(&mut self, filename: &str, hash: &Sha256Hash) {
        self.name_to_hash_map.insert(filename.to_owned(), hash.clone());
    }
}
