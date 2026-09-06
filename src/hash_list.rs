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
use std::path::Path;
use std::{
    fs::File,
    io::{BufRead, BufReader, BufWriter, Write},
};
use tempfile::NamedTempFile;

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
            // Skip blank lines. A truncated or partially written file can end in one,
            // and an empty line splits into an empty first field that is not a hash.
            if line.trim().is_empty() {
                continue;
            }
            let mut spi = line.split(' ');
            let (Some(hash_str), Some(name)) = (spi.next(), spi.next()) else {
                continue;
            };
            let hash = Sha256Hash::from_hex(hash_str).map_err(|()| TaError::BadHashFormat)?;
            name_to_hash_map.insert(name.to_owned(), hash);
        }
        Ok(Self { name_to_hash_map })
    }

    /// Save hash list to a file
    ///
    /// The write goes to a temporary file that then replaces the target, because
    /// concurrent tests can read this file while another process writes it.
    /// A direct write truncates the file and makes readers see partial data.
    pub fn to_file(&self, path: &str) -> Result<(), TaError> {
        let path = Path::new(path);
        let dir = path.parent().unwrap_or(Path::new("."));
        let temp = NamedTempFile::new_in(dir)?;
        {
            let mut bwrtr = BufWriter::new(&temp);
            self.to_writer(&mut bwrtr)?;
            bwrtr.flush()?;
        }
        temp.persist(path).map_err(|e| e.error)?;
        Ok(())
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

#[cfg(test)]
mod tests {
    use super::*;

    const HASH_A: &str = "976c1638d8c1ba8014de6c64b196cbd70a5acf031be10a8e7f649536193c8e78";
    const HASH_B: &str = "9d9f5ba77b562fd4141fc725038028822673b24595e2774a8718260f4fc39710";

    fn parse(text: &str) -> Result<HashList, TaError> {
        HashList::from_reader(&mut text.as_bytes())
    }

    #[test]
    fn reads_entries_and_skips_comments() {
        let list = parse(&format!("# a comment\n{HASH_A} a/one.bin\n{HASH_B} b/two.bin\n"))
            .expect("must parse");

        assert_eq!(list.get_hash("a/one.bin"), Some(&Sha256Hash::from_hex(HASH_A).unwrap()));
        assert_eq!(list.get_hash("b/two.bin"), Some(&Sha256Hash::from_hex(HASH_B).unwrap()));
        assert_eq!(list.get_hash("missing.bin"), None);
    }

    /// A truncated file can end in a blank line. This must not be an error.
    #[test]
    fn skips_blank_lines() {
        let list = parse(&format!("\n{HASH_A} a/one.bin\n\n   \n")).expect("must parse");

        assert_eq!(list.get_hash("a/one.bin"), Some(&Sha256Hash::from_hex(HASH_A).unwrap()));
    }

    /// A partial write can cut a line before its name. Such a line has no entry to add.
    #[test]
    fn skips_line_that_has_no_name() {
        let list = parse(&format!("{HASH_A} a/one.bin\n{HASH_B}")).expect("must parse");

        assert_eq!(list.get_hash("a/one.bin"), Some(&Sha256Hash::from_hex(HASH_A).unwrap()));
        assert_eq!(list.get_hash("b/two.bin"), None);
    }

    #[test]
    fn rejects_a_hash_that_is_not_hexadecimal() {
        let result = parse("zzzz not-a-hash.bin\n");

        assert!(matches!(result, Err(TaError::BadHashFormat)));
    }

    #[test]
    fn writes_then_reads_the_same_entries() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("hash_list");
        let path = path.to_str().unwrap();

        let mut written = HashList::new();
        written.add_entry("a/one.bin", &Sha256Hash::from_hex(HASH_A).unwrap());
        written.to_file(path).expect("must write");

        let read = HashList::from_file(path).expect("must read");
        assert_eq!(read.get_hash("a/one.bin"), Some(&Sha256Hash::from_hex(HASH_A).unwrap()));
    }

    /// A reader must never see a partial file while a writer replaces it.
    #[test]
    fn reads_stay_valid_while_a_writer_replaces_the_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("hash_list");
        let path = path.to_str().unwrap();

        let mut list = HashList::new();
        for i in 0..200 {
            list.add_entry(&format!("dir/file_{i}.bin"), &Sha256Hash::from_hex(HASH_A).unwrap());
        }
        list.to_file(path).expect("must write");

        std::thread::scope(|scope| {
            scope.spawn(|| {
                for _ in 0..50 {
                    list.to_file(path).expect("must write");
                }
            });
            scope.spawn(|| {
                for _ in 0..50 {
                    let read = HashList::from_file(path).expect("must read a complete file");
                    // The file always holds every entry, so a reader that finds a
                    // missing entry saw a partial write.
                    for i in 0..200 {
                        assert!(
                            read.get_hash(&format!("dir/file_{i}.bin")).is_some(),
                            "reader saw a partially written file"
                        );
                    }
                }
            });
        });
    }
}
