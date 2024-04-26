use std::sync::{Arc, RwLock};

use crate::{
    config::{AliasConfig, ApiKeyConfig},
    format::ChatRequest,
    providers::ChatModelProvider,
    Error, ModelLookupChoice, ModelLookupResult, ProxyRequestOptions,
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

    fn lookup_api_key(&self, name: &str) -> Option<String> {
        self.api_keys
            .iter()
            .find(|key| key.name == name)
            .map(|key| key.value.clone())
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
        if !options.models.is_empty() {
            let lookup = self.0.read().unwrap();
            let choices = options
                .models
                .iter()
                .map(|choice| {
                    let provider = lookup
                        .providers
                        .iter()
                        .find(|p| p.name() == choice.provider)
                        .ok_or_else(|| Error::UnknownProvider(choice.provider.to_string()))?
                        .clone();

                    let api_key = match (&choice.api_key, &choice.api_key_name) {
                        (Some(key), _) => Some(key.clone()),
                        (None, Some(key_name)) => {
                            let key = lookup
                                .lookup_api_key(key_name)
                                .ok_or_else(|| Error::NoApiKey(key_name.to_string()))?;
                            Some(key)
                        }
                        (None, None) => None,
                    };

                    Ok::<ModelLookupChoice, Error>(ModelLookupChoice {
                        model: choice.model.clone(),
                        provider,
                        api_key,
                    })
                })
                .collect::<Result<Vec<_>, Error>>()?;

            return Ok(ModelLookupResult {
                alias: String::new(),
                random_order: options.random_choice,
                choices,
            });
        }

        let model = if let Some(model) = &options.model {
            model.as_str()
        } else {
            body.model.as_deref().unwrap_or_default()
        };

        if model.is_empty() {
            return Err(Error::ModelNotSpecified);
        }

        let lookup = self.0.read().unwrap();
        let alias = lookup.aliases.iter().find(|alias| alias.name == model);

        let choices = if let Some(alias) = alias {
            alias
                .models
                .iter()
                .map(|choice| {
                    let provider = lookup
                        .providers
                        .iter()
                        .find(|p| p.name() == choice.provider)
                        .ok_or_else(|| {
                            Error::NoAliasProvider(alias.name.clone(), choice.provider.clone())
                        })?
                        .clone();

                    let api_key = if let Some(key_name) = &choice.api_key_name {
                        let api_key = lookup.lookup_api_key(key_name).ok_or_else(|| {
                            Error::NoAliasApiKey(alias.name.clone(), key_name.to_string())
                        })?;
                        Some(api_key)
                    } else {
                        None
                    };
                    Ok::<_, Error>(ModelLookupChoice {
                        model: choice.model.clone(),
                        provider,
                        api_key,
                    })
                })
                .into_iter()
                .collect::<Result<Vec<_>, Error>>()?
        } else if let Some(provider_name) = options.provider.as_deref() {
            let provider = lookup
                .get_provider(provider_name)
                .ok_or_else(|| Error::UnknownProvider(provider_name.to_string()))?;

            vec![ModelLookupChoice {
                model: model.to_string(),
                provider,
                api_key: options.api_key.clone(),
            }]
        } else {
            let provider = lookup
                .default_provider_for_model(model)
                .ok_or_else(|| Error::NoDefault(model.to_string()))?;

            vec![ModelLookupChoice {
                model: model.to_string(),
                provider,
                api_key: options.api_key.clone(),
            }]
        };

        Ok(ModelLookupResult {
            alias: alias.map(|a| a.name.clone()).unwrap_or_default(),
            random_order: alias.map(|a| a.random_order).unwrap_or(false),
            choices,
        })
    }
}
