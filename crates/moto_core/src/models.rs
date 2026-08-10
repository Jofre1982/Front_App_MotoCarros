//! Tipos que reflejan el contrato JSON de `/api/v1`.
//!
//! Se completan a medida que se implementan los issues correspondientes — no
//! se inventan campos que el backend no exponga todavia.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AuthToken {
    pub token: String,
}
