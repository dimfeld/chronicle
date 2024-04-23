use std::sync::{Arc, RwLock};

use crate::{
    config::{AliasConfig, ApiKeyConfig},
    format::ChatRequest,
    providers::ChatModelProvider,
    Error, ModelLookupResult, ProxyRequestOptions,
};

#[derive(Debug)]
struct ProviderLookupInternal {
    providers: Vec<Arc<dyn ChatModelProvider>>,
    aliases: Vec<AliasConfig>,
    api_keys: Vec<ApiKeyConfig>,
}

impl ProviderLookupInternal {
    fn get_provider(&self, name: &str) -> Option<Arc<dyn ChatModelProvider>> {
        self.providers
            .iter()
            .find(|p| p.name() == name)
            .map(Arc::clone)
    }

    fn default_provider_for_model(&self, model: &str) -> Option<Arc<dyn ChatModelProvider>> {
        self.providers
            .iter()
            .find(|p| p.is_default_for_model(model))
            .map(Arc::clone)
    }
}

#[derive(Debug)]
pub struct ProviderLookup(RwLock<ProviderLookupInternal>);

impl ProviderLookup {
    pub fn new(
        providers: Vec<Arc<dyn ChatModelProvider>>,
        aliases: Vec<AliasConfig>,
        api_keys: Vec<ApiKeyConfig>,
    ) -> Self {
        Self(RwLock::new(ProviderLookupInternal {
            providers,
            aliases,
            api_keys,
        }))
    }

    pub fn get_provider(&self, name: &str) -> Option<Arc<dyn ChatModelProvider>> {
        self.0.read().unwrap().get_provider(name)
    }

    pub fn default_provider_for_model(&self, model: &str) -> Option<Arc<dyn ChatModelProvider>> {
        self.0.read().unwrap().default_provider_for_model(model)
    }

    pub fn find_model_and_provider<'a>(
        &self,
        options: &'a ProxyRequestOptions,
        body: &'a ChatRequest,
    ) -> Result<ModelLookupResult, Error> {
        let (from_options, model) = if let Some(model) = &options.model {
            (true, model.as_str())
        } else {
            (false, body.model.as_deref().unwrap_or_default())
        };

        if model.is_empty() {
            return Err(Error::ModelNotSpecified);
        }

        let lookup = self.0.read().unwrap();
        let alias = lookup.aliases.iter().find(|alias| alias.name == model);

        let provider = if let Some(alias) = alias {
            lookup
                .providers
                .iter()
                .find(|p| p.name() == alias.provider)
                .ok_or_else(|| Error::NoAliasProvider(alias.name.clone(), alias.provider.clone()))?
                .clone()
        } else if let Some(provider_name) = options.provider.as_deref() {
            lookup
                .get_provider(provider_name)
                .ok_or_else(|| Error::UnknownProvider(provider_name.to_string()))?
        } else {
            lookup
                .default_provider_for_model(model)
                .ok_or_else(|| Error::NoDefault(model.to_string()))?
        };

        let model = alias.map(|alias| alias.model.as_str()).unwrap_or(model);

        let api_key = alias
            .and_then(|alias| {
                if let Some(key_name) = alias.api_key_name.as_deref() {
                    let key = lookup
                        .api_keys
                        .iter()
                        .find(|key| key.name == key_name)
                        .map(|key| key.value.clone())
                        .ok_or_else(|| {
                            Error::NoAliasApiKey(alias.name.clone(), key_name.to_string())
                        });

                    Some(key)
                } else {
                    None
                }
            })
            .transpose()?;

        Ok(ModelLookupResult {
            from_options: from_options || alias.is_some(),
            provider,
            model: model.to_string(),
            api_key,
        })
    }
}
