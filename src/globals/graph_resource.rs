use std::sync::{Arc, RwLock};

use crate::graph::Graph;

#[derive(Clone, Debug)]
pub struct GraphResource(pub Arc<RwLock<Graph>>);

impl Default for GraphResource {
    fn default() -> Self {
        Self(Arc::new(RwLock::new(Graph::default())))
    }
}

impl serde::Serialize for GraphResource {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        // println!("serialize");
        self.0.read().unwrap().serialize(serializer)
    }
}

impl<'de> serde::Deserialize<'de> for GraphResource {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let result = Graph::deserialize(deserializer);
        // println!("result: {:?}", result);
        Ok(Self(Arc::new(RwLock::new(result.unwrap()))))
    }
}

impl GraphResource {
    pub fn with_graph<T>(&self, f: impl FnOnce(&mut Graph) -> T) -> T {
        let mut graph = self.0.write().unwrap();
        f(&mut graph)
    }

    pub fn read_graph<T>(&self, f: impl FnOnce(&Graph) -> T) -> T {
        let graph = self.0.read().unwrap();
        f(&graph)
    }
}
