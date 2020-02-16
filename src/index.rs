use serde::{Deserialize, Serialize};

use cosmwasm::errors::Result;
use cosmwasm::traits::{ReadonlyStorage, Storage};

use crate::namespace_helpers::key_prefix;
use crate::typed::{typed, typed_read};

pub fn index<T, F>(namespace: &[u8], action: F) -> Index<T>
    where F: Fn(&T) -> Vec<u8> + 'static {
    Index {
        prefix: key_prefix(namespace),
        action: Box::new(action),
    }
}

pub struct Index<T> {
    prefix: Vec<u8>,
    action: Box<dyn Fn(&T) -> Vec<u8>>,
}

impl<T> Index<T> {
    fn calc_key(&self, item: &T) -> Vec<u8> {
        let calc = (self.action)(item);
        let mut k = self.prefix.clone();
        k.extend_from_slice(&calc);
        k
    }
}


/// IndexEntry is persisted to disk and lists all primary keys that have a given index value
#[derive(Serialize, Deserialize, PartialEq, Debug, Clone, Default)]
struct IndexEntry {
    // TODO: make this Vec<Base64> in 0.7.0
    pub refs: Vec<Vec<u8>>,
}

/*
This is getting expensive.
Saving an item without index is 1 write
Creating an item with 1 index is 2 read + 2 write (1 read to check old value, 1 read+write to add_key)
Updating an item with 1 index is 3 read + 3 write (1 read to check old value, 1 read+write to add_key, 1 read+write to remove_key)

It *may* be possible to reduce the number of reads, but writes cannot change
*/

// must do a read for old data
fn write_index<S: Storage, T>(storage: &mut S, idx: &Index<T>, pk: &[u8], old_val: Option<&T>, new_val: &T) -> Result<()> {
    let old_idx = old_val.map(|o| idx.calc_key(o));
    let new_idx = idx.calc_key(new_val);

    // no change is a no-op
    if let Some(o) = &old_idx {
        // if it unchanged, it is a no-op
        if o == &new_idx {
            return Ok(());
        }
        // otherwise, remove it
        remove_key(storage, o.as_slice(), pk)?;
    }

    // now add the new pk
    add_key(storage, new_idx.as_slice(), pk)
}

fn remove_key<S: Storage>(storage: &mut S, idx: &[u8], pk: &[u8]) -> Result<()> {
    let mut db = typed(storage);
    let mut entry: IndexEntry = db.load(idx)?;
    // TODO: error if not found?
    entry.refs = entry.refs.into_iter().filter(|r| r.as_slice() != pk).collect();
    db.save(idx, &entry)
}

fn add_key<S: Storage>(storage: &mut S, idx: &[u8], pk: &[u8]) -> Result<()> {
    let mut db = typed(storage);
    let mut entry: IndexEntry = db.may_load(idx)?.unwrap_or_default();
    entry.refs.push(pk.to_vec());
    db.save(idx, &entry)
}

fn load_keys<S: ReadonlyStorage>(storage: &S, idx: &[u8]) -> Result<Option<IndexEntry>> {
    let db = typed_read(storage);
    db.may_load(idx)
}

#[cfg(test)]
mod test {
    use super::*;
    use serde::{Deserialize, Serialize};

    #[derive(Serialize, Deserialize, PartialEq, Debug)]
    struct Person {
        pub name: String,
        pub age: u32,
    }

    #[test]
    fn build_index() {
        let idx = index(b"foo", |p: &Person| p.age.to_be_bytes().to_vec());

        let expected = vec![0u8, 3, b'f', b'o', b'o', 0, 0, 0, 127];
        let trial = idx.calc_key(&Person{ name: "Fred".to_string(), age: 127 });
        assert_eq!(trial, expected);
    }
}
