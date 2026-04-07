//! Pulumi code-generation backend for `iac-forge`.
//!
//! This crate translates the `iac-forge` intermediate representation into a
//! Pulumi Package Schema (`schema.json`).

mod backend;
mod schema;

pub use backend::PulumiBackend;
pub use schema::{
    ComplexType, EnumValue, FunctionSchema, ObjectTypeSpec, PropertySpec, ProviderResource,
    PulumiSchema, ResourceSchema,
};
