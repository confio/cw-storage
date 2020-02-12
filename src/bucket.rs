use named_type::NamedType;
use serde::{de::DeserializeOwned, ser::Serialize};
use std::marker::PhantomData;

use cosmwasm::errors::Result;
use cosmwasm::traits::{ReadonlyStorage, Storage};

use crate::namespace_helpers::{get_with_prefix, key_prefix, key_prefix_nested, set_with_prefix};
use crate::type_helpers::{may_deserialize, must_deserialize, serialize};

pub fn bucket<'a, S: Storage, T>(namespace: &[u8], storage: &'a mut S) -> Bucket<'a, S, T>
where
    T: Serialize + DeserializeOwned + NamedType,
{
    Bucket::new(namespace, storage)
}

pub fn bucket_read<'a, S: ReadonlyStorage, T>(
    namespace: &[u8],
    storage: &'a S,
) -> ReadonlyBucket<'a, S, T>
where
    T: Serialize + DeserializeOwned + NamedType,
{
    ReadonlyBucket::new(namespace, storage)
}

pub struct Bucket<'a, S: Storage, T>
where
    T: Serialize + DeserializeOwned + NamedType,
{
    storage: &'a mut S,
    // see https://doc.rust-lang.org/std/marker/struct.PhantomData.html#unused-type-parameters for why this is needed
    data: PhantomData<&'a T>,
    prefix: Vec<u8>,
}

impl<'a, S: Storage, T> Bucket<'a, S, T>
where
    T: Serialize + DeserializeOwned + NamedType,
{
    pub fn new(namespace: &[u8], storage: &'a mut S) -> Self {
        Bucket {
            prefix: key_prefix(namespace),
            storage,
            data: PhantomData,
        }
    }

    pub fn multilevel(namespaces: &[&[u8]], storage: &'a mut S) -> Self {
        Bucket {
            prefix: key_prefix_nested(namespaces),
            storage,
            data: PhantomData,
        }
    }

    /// save will serialize the model and store, returns an error on serialization issues
    pub fn save(&mut self, key: &[u8], data: &T) -> Result<()> {
        set_with_prefix(self.storage, &self.prefix, key, &serialize(data)?);
        Ok(())
    }

    /// load will return an error if no data is set at the given key, or on parse error
    pub fn load(&self, key: &[u8]) -> Result<T> {
        let value = get_with_prefix(self.storage, &self.prefix, key);
        must_deserialize(&value)
    }

    /// may_load will parse the data stored at the key if present, returns Ok(None) if no data there.
    /// returns an error on issues parsing
    pub fn may_load(&self, key: &[u8]) -> Result<Option<T>> {
        let value = get_with_prefix(self.storage, &self.prefix, key);
        may_deserialize(&value)
    }

    /// update will load the data, perform the specified action, and store the result
    /// in the database. This is shorthand for some common sequences, which may be useful.
    /// Note that this only updates *pre-existing* values. If you want to modify possibly
    /// non-existent values, please use `may_update`
    ///
    /// This is the least stable of the APIs, and definitely needs some usage
    pub fn update(&mut self, key: &[u8], action: &dyn Fn(T) -> Result<T>) -> Result<T> {
        let input = self.load(key)?;
        let output = action(input)?;
        self.save(key, &output)?;
        Ok(output)
    }

    /// may_update is like update, but can handle missing values:
    /// * If there is no data at this key, the input is None
    /// * We don't save data if the action returns None
    ///
    /// This is the least stable of the APIs, and definitely needs some usage
    pub fn may_update(
        &mut self,
        key: &[u8],
        action: &dyn Fn(Option<T>) -> Result<Option<T>>,
    ) -> Result<Option<T>> {
        let input = self.may_load(key)?;
        let output = action(input)?;
        if let Some(data) = &output {
            self.save(key, data)?;
        }
        Ok(output)
    }
}

pub struct ReadonlyBucket<'a, S: ReadonlyStorage, T>
where
    T: Serialize + DeserializeOwned + NamedType,
{
    storage: &'a S,
    // see https://doc.rust-lang.org/std/marker/struct.PhantomData.html#unused-type-parameters for why this is needed
    data: PhantomData<&'a T>,
    prefix: Vec<u8>,
}

impl<'a, S: ReadonlyStorage, T> ReadonlyBucket<'a, S, T>
where
    T: Serialize + DeserializeOwned + NamedType,
{
    pub fn new(namespace: &[u8], storage: &'a S) -> Self {
        ReadonlyBucket {
            prefix: key_prefix(namespace),
            storage,
            data: PhantomData,
        }
    }

    pub fn multilevel(namespaces: &[&[u8]], storage: &'a S) -> Self {
        ReadonlyBucket {
            prefix: key_prefix_nested(namespaces),
            storage,
            data: PhantomData,
        }
    }

    /// load will return an error if no data is set at the given key, or on parse error
    pub fn load(&self, key: &[u8]) -> Result<T> {
        let value = get_with_prefix(self.storage, &self.prefix, key);
        must_deserialize(&value)
    }

    /// may_load will parse the data stored at the key if present, returns Ok(None) if no data there.
    /// returns an error on issues parsing
    pub fn may_load(&self, key: &[u8]) -> Result<Option<T>> {
        let value = get_with_prefix(self.storage, &self.prefix, key);
        may_deserialize(&value)
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use cosmwasm::errors::ContractErr;
    use cosmwasm::mock::MockStorage;
    use named_type_derive::NamedType;
    use serde::{Deserialize, Serialize};

    #[derive(Serialize, Deserialize, NamedType, PartialEq, Debug)]
    struct Data {
        pub name: String,
        pub age: i32,
    }

    #[test]
    fn store_and_load() {
        let mut store = MockStorage::new();
        let mut bucket = bucket::<_, Data>(b"data", &mut store);

        // save data
        let data = Data {
            name: "Maria".to_string(),
            age: 42,
        };
        bucket.save(b"maria", &data).unwrap();

        // load it properly
        let loaded = bucket.load(b"maria").unwrap();
        assert_eq!(data, loaded);
    }

    #[test]
    fn readonly_works() {
        let mut store = MockStorage::new();
        let mut bucket = bucket::<_, Data>(b"data", &mut store);

        // save data
        let data = Data {
            name: "Maria".to_string(),
            age: 42,
        };
        bucket.save(b"maria", &data).unwrap();

        let reader = bucket_read::<_, Data>(b"data", &mut store);

        // check empty data handling
        assert!(reader.load(b"john").is_err());
        assert_eq!(reader.may_load(b"john").unwrap(), None);

        // load it properly
        let loaded = reader.load(b"maria").unwrap();
        assert_eq!(data, loaded);
    }

    #[test]
    fn buckets_isolated() {
        let mut store = MockStorage::new();
        let mut bucket1 = bucket::<_, Data>(b"data", &mut store);

        // save data
        let data = Data {
            name: "Maria".to_string(),
            age: 42,
        };
        bucket1.save(b"maria", &data).unwrap();

        let mut bucket2 = bucket::<_, Data>(b"dat", &mut store);

        // save data (dat, amaria) vs (data, maria)
        let data2 = Data {
            name: "Amen".to_string(),
            age: 67,
        };
        bucket2.save(b"amaria", &data2).unwrap();

        // load one
        let reader = bucket_read::<_, Data>(b"data", &store);
        let loaded = reader.load(b"maria").unwrap();
        assert_eq!(data, loaded);
        // no cross load
        assert_eq!(None, reader.may_load(b"amaria").unwrap());

        // load the other
        let reader2 = bucket_read::<_, Data>(b"dat", &store);
        let loaded2 = reader2.load(b"amaria").unwrap();
        assert_eq!(data2, loaded2);
        // no cross load
        assert_eq!(None, reader2.may_load(b"maria").unwrap());
    }

    #[test]
    fn update_success() {
        let mut store = MockStorage::new();
        let mut bucket = bucket::<_, Data>(b"data", &mut store);

        // initial data
        let init = Data {
            name: "Maria".to_string(),
            age: 42,
        };
        bucket.save(b"maria", &init).unwrap();

        // it's my birthday
        let birthday = |mut d: Data| {
            d.age += 1;
            Ok(d)
        };
        let output = bucket.update(b"maria", &birthday).unwrap();
        let expected = Data {
            name: "Maria".to_string(),
            age: 43,
        };
        assert_eq!(output, expected);

        // load it properly
        let loaded = bucket.load(b"maria").unwrap();
        assert_eq!(loaded, expected);
    }

    #[test]
    fn update_fails_on_error() {
        let mut store = MockStorage::new();
        let mut bucket = bucket::<_, Data>(b"data", &mut store);

        // initial data
        let init = Data {
            name: "Maria".to_string(),
            age: 42,
        };
        bucket.save(b"maria", &init).unwrap();

        // it's my birthday
        let output = bucket.update(b"maria", &|_d| {
            ContractErr {
                msg: "cuz i feel like it",
            }
            .fail()
        });
        assert!(output.is_err());

        // load it properly
        let loaded = bucket.load(b"maria").unwrap();
        assert_eq!(loaded, init);
    }

    #[test]
    fn update_fails_on_no_data() {
        let mut store = MockStorage::new();
        let mut bucket = bucket::<_, Data>(b"data", &mut store);

        // it's my birthday
        let output = bucket.update(b"maria", &|mut d| {
            d.age += 1;
            Ok(d)
        });
        assert!(output.is_err());

        // nothing stored
        let loaded = bucket.may_load(b"maria").unwrap();
        assert_eq!(loaded, None);
    }

    #[test]
    fn may_update_handles_none() {
        let mut store = MockStorage::new();
        let mut bucket = bucket::<_, Data>(b"data", &mut store);

        // only set first time
        let val = bucket
            .may_update(b"first", &|t| match t {
                Some(_) => Ok(None),
                None => Ok(Some(Data {
                    name: "Maria".to_string(),
                    age: 42,
                })),
            })
            .unwrap();
        assert!(val.is_some());

        // ensure we get the data
        let loaded = bucket.load(b"first").unwrap();
        assert_eq!(loaded.age, 42);
        assert_eq!(loaded.name.as_str(), "Maria");

        // update with same function (don't change set values)
        // only set first time
        let val = bucket
            .may_update(b"first", &|t| match t {
                Some(_) => Ok(None),
                None => Ok(Some(Data {
                    name: "Joe".to_string(),
                    age: 27,
                })),
            })
            .unwrap();
        assert!(val.is_none());

        // ensure data was not modified
        let loaded = bucket.load(b"first").unwrap();
        assert_eq!(loaded.age, 42);
        assert_eq!(loaded.name.as_str(), "Maria");
    }
}
