use serde_json::Value;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::Arc;

#[derive(Clone)]
pub enum GlobalMetadata {
    Static(Value),
    Supplier(Arc<dyn Fn() -> Value + Send + Sync + 'static>),
}

impl std::fmt::Debug for GlobalMetadata {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Static(value) => f.debug_tuple("Static").field(value).finish(),
            Self::Supplier(_) => f.write_str("Supplier(<fn>)"),
        }
    }
}

impl GlobalMetadata {
    pub fn static_map(value: impl Into<Value>) -> Self {
        Self::Static(value.into())
    }

    pub fn supplier<F>(supplier: F) -> Self
    where
        F: Fn() -> Value + Send + Sync + 'static,
    {
        Self::Supplier(Arc::new(supplier))
    }

    pub(crate) fn read(&self) -> Option<Value> {
        match self {
            Self::Static(value) => Some(value.clone()),
            Self::Supplier(supplier) => catch_unwind(AssertUnwindSafe(|| supplier())).ok(),
        }
    }
}
