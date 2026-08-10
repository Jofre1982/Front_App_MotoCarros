//! Cliente HTTP hacia `Back_App_MotoCarros` (`/api/v1`).
//!
//! Placeholder de bootstrap: los endpoints reales se agregan issue por issue,
//! reflejando el contrato que expone el backend en cada momento.

pub struct ApiClient {
    pub base_url: String,
}

impl ApiClient {
    pub fn new(base_url: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_stores_base_url() {
        let client = ApiClient::new("https://api.example.com");
        assert_eq!(client.base_url, "https://api.example.com");
    }
}
