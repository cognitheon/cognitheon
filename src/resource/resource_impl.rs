use std::sync::{Arc, RwLock};

// use crate::{canvas::CanvasState, graph::Graph};

// pub mod particle_system_resource;

#[derive(Debug)]
pub struct Resource<T>(pub Arc<RwLock<T>>);

impl<T> Default for Resource<T>
where
    T: Default,
{
    fn default() -> Self {
        Self(Arc::new(RwLock::new(T::default())))
    }
}

impl<T> Clone for Resource<T> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

impl<T> serde::Serialize for Resource<T>
where
    T: serde::Serialize,
{
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        self.0.read().unwrap().serialize(serializer)
    }
}

impl<'de, T> serde::Deserialize<'de> for Resource<T>
where
    T: serde::de::DeserializeOwned,
{
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        Ok(Self(Arc::new(RwLock::new(T::deserialize(deserializer)?))))
    }
}

impl<T> Resource<T> {
    pub fn new(value: T) -> Self {
        Self(Arc::new(RwLock::new(value)))
    }

    pub fn read_resource<R>(&self, f: impl FnOnce(&T) -> R) -> R {
        let reader = self.0.read().unwrap();
        f(&reader)
    }

    pub fn with_resource<R>(&self, f: impl FnOnce(&mut T) -> R) -> R {
        let mut writer = self.0.write().unwrap();
        f(&mut writer)
    }
}
