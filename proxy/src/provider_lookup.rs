use std::{
    collections::HashMap,
    sync::{Arc, RwLock},
};

use crate::{
    config::{AliasConfig, ApiKeyConfig},
    format::ChatRequest,
    providers::ChatModelProvider,
    Error, ProxyRequestOptions,
};

#[derive(Debug)]
pub struct ModelLookupResult {
    pub alias: String,
    pub random_order: bool,
    pub choices: Vec<ModelLookupChoice>,
}

#[derive(Debug)]
pub struct ModelLookupChoice {
    pub model: String,
    pub provider: Arc<dyn ChatModelProvider>,
    pub api_key: Option<String>,
}

#[derive(Debug)]
struct ProviderLookupInternal {
    providers: HashMap<String, Arc<dyn ChatModelProvider>>,
    aliases: HashMap<String, AliasConfig>,
    api_keys: HashMap<String, ApiKeyConfig>,
}

impl ProviderLookupInternal {
    fn get_provider(&self, name: &str) -> Option<Arc<dyn ChatModelProvider>> {
        self.providers.get(name).map(Arc::clone)
    }

    fn default_provider_for_model(&self, model: &str) -> Option<Arc<dyn ChatModelProvider>> {
        self.providers
            .iter()
            .find(|(_, p)| p.is_default_for_model(model))
            .map(|(_, p)| Arc::clone(p))
    }

    fn lookup_api_key(&self, name: &str) -> Option<String> {
        self.api_keys.get(name).map(|key| key.value.clone())
    }
}

#[derive(Debug)]
pub(crate) struct ProviderLookup(RwLock<ProviderLookupInternal>);

impl ProviderLookup {
    pub fn new(
        providers: Vec<Arc<dyn ChatModelProvider>>,
        aliases: Vec<AliasConfig>,
        api_keys: Vec<ApiKeyConfig>,
    ) -> Self {
        let providers = providers
            .into_iter()
            .map(|p| (p.name().to_string(), p))
            .collect();

        let aliases = aliases
            .into_iter()
            .map(|a| (a.name.to_string(), a))
            .collect();

        let api_keys = api_keys
            .into_iter()
            .map(|a| (a.name.to_string(), a))
            .collect();

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
                        .get(&choice.provider)
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
        let alias = lookup.aliases.get(model);

        let choices = if let Some(alias) = alias {
            alias
                .models
                .iter()
                .map(|choice| {
                    let provider = lookup
                        .providers
                        .get(&choice.provider)
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

    /// Add a provider to the system. This will replace any existing provider with the same `name`.
    pub fn set_provider(&self, provider: Arc<dyn ChatModelProvider>) {
        let name = provider.name().to_string();
        self.0.write().unwrap().providers.insert(name, provider);
    }

    /// Remove a provider
    pub fn remove_provider(&self, name: &str) {
        self.0.write().unwrap().providers.remove(name);
    }

    /// Add an alias to the system. This will replace any existing alias with the same `name`.
    pub fn set_alias(&self, alias: AliasConfig) {
        self.0
            .write()
            .unwrap()
            .aliases
            .insert(alias.name.clone(), alias);
    }

    /// Remove an alias
    pub fn remove_alias(&self, name: &str) {
        self.0.write().unwrap().aliases.remove(name);
    }

    /// Add an API key to the system. This will replace any existing API key with the same `name`.
    pub fn set_api_key(&self, api_key: ApiKeyConfig) {
        self.0
            .write()
            .unwrap()
            .api_keys
            .insert(api_key.name.clone(), api_key);
    }

    /// Remove an API key
    pub fn remove_api_key(&self, name: &str) {
        self.0.write().unwrap().api_keys.remove(name);
    }

    pub(crate) fn validate(&self) -> Vec<String> {
        todo!();
        vec![]
    }
}

#[cfg(test)]
mod test {
    use std::sync::Arc;

    use super::ProviderLookup;
    use crate::{
        config::{AliasConfig, AliasConfigProvider, ApiKeyConfig},
        format::ChatRequest,
        providers::ChatModelProvider,
        Error, ModelAndProvider, ProxyRequestOptions,
    };

    fn generate_lookup() -> ProviderLookup {
        let client = reqwest::Client::new();
        let providers = vec![
            Arc::new(crate::providers::openai::OpenAi::new(client.clone(), None))
                as Arc<dyn ChatModelProvider>,
            Arc::new(crate::providers::anthropic::Anthropic::new(client, None))
                as Arc<dyn ChatModelProvider>,
        ];

        let aliases = vec![
            AliasConfig {
                name: "alias-1".to_string(),
                random_order: false,
                models: vec![
                    AliasConfigProvider {
                        model: "model-1".to_string(),
                        provider: "openai".to_string(),
                        api_key_name: Some("key-1".to_string()),
                    },
                    AliasConfigProvider {
                        model: "model-2".to_string(),
                        provider: "anthropic".to_string(),
                        api_key_name: None,
                    },
                ],
            },
            AliasConfig {
                name: "alias-2".to_string(),
                random_order: true,
                models: vec![
                    AliasConfigProvider {
                        model: "model-1".to_string(),
                        provider: "openai".to_string(),
                        api_key_name: None,
                    },
                    AliasConfigProvider {
                        model: "model-2".to_string(),
                        provider: "anthropic".to_string(),
                        api_key_name: Some("key-1".to_string()),
                    },
                ],
            },
            AliasConfig {
                name: "bad-provider-alias".to_string(),
                random_order: false,
                models: vec![
                    AliasConfigProvider {
                        model: "model-1".to_string(),
                        provider: "openai".to_string(),
                        api_key_name: None,
                    },
                    AliasConfigProvider {
                        model: "model-2".to_string(),
                        provider: "no-provider".to_string(),
                        api_key_name: None,
                    },
                ],
            },
            AliasConfig {
                name: "bad-key-alias".to_string(),
                random_order: false,
                models: vec![
                    AliasConfigProvider {
                        model: "model-1".to_string(),
                        provider: "openai".to_string(),
                        api_key_name: Some("no-key".to_string()),
                    },
                    AliasConfigProvider {
                        model: "model-2".to_string(),
                        provider: "no-provider".to_string(),
                        api_key_name: None,
                    },
                ],
            },
        ];

        let api_keys = vec![ApiKeyConfig {
            name: "key-1".to_string(),
            source: String::new(),
            value: "actual-key-1-key".to_string(),
        }];

        ProviderLookup::new(providers, aliases, api_keys)
    }

    #[test]
    fn supplied_choices() {
        let lookup = generate_lookup();
        let result = lookup
            .find_model_and_provider(
                &ProxyRequestOptions {
                    random_choice: true,
                    models: vec![
                        ModelAndProvider {
                            model: "abc".to_string(),
                            provider: "openai".to_string(),
                            api_key_name: Some("key-1".to_string()),
                            api_key: Some("keykey".to_string()),
                        },
                        ModelAndProvider {
                            model: "def".to_string(),
                            provider: "anthropic".to_string(),
                            api_key_name: Some("key-1".to_string()),
                            api_key: None,
                        },
                    ],
                    ..Default::default()
                },
                &ChatRequest {
                    model: Some("body-model".to_string()),
                    ..Default::default()
                },
            )
            .expect("lookup should succeed");

        assert_eq!(result.alias, "");
        assert_eq!(result.random_order, true);
        assert_eq!(result.choices.len(), 2);

        assert_eq!(result.choices[0].model, "abc");
        assert_eq!(result.choices[0].provider.name(), "openai");
        // This should overide the key name since it was explicitly requested
        assert_eq!(result.choices[0].api_key, Some("keykey".to_string()));

        assert_eq!(result.choices[1].model, "def");
        assert_eq!(result.choices[1].provider.name(), "anthropic");
        assert_eq!(
            result.choices[1].api_key,
            Some("actual-key-1-key".to_string())
        );
    }

    #[test]
    fn supplied_choices_nonexistent_provider() {
        let lookup = generate_lookup();
        let result = lookup
            .find_model_and_provider(
                &ProxyRequestOptions {
                    random_choice: true,
                    models: vec![
                        ModelAndProvider {
                            model: "abc".to_string(),
                            provider: "openai".to_string(),
                            api_key_name: Some("key-1".to_string()),
                            api_key: Some("keykey".to_string()),
                        },
                        ModelAndProvider {
                            model: "def".to_string(),
                            provider: "ooo".to_string(),
                            api_key_name: Some("key-1".to_string()),
                            api_key: None,
                        },
                    ],
                    ..Default::default()
                },
                &ChatRequest {
                    model: Some("body-model".to_string()),
                    ..Default::default()
                },
            )
            .expect_err("lookup should fail");

        assert!(matches!(result, Error::UnknownProvider(_)));
    }

    #[test]
    fn supplied_choices_nonexistent_api_key_name() {
        let lookup = generate_lookup();
        let result = lookup
            .find_model_and_provider(
                &ProxyRequestOptions {
                    random_choice: true,
                    models: vec![
                        ModelAndProvider {
                            model: "abc".to_string(),
                            provider: "openai".to_string(),
                            api_key_name: Some("key-1".to_string()),
                            api_key: Some("keykey".to_string()),
                        },
                        ModelAndProvider {
                            model: "def".to_string(),
                            provider: "anthropic".to_string(),
                            api_key_name: Some("no-key".to_string()),
                            api_key: None,
                        },
                    ],
                    ..Default::default()
                },
                &ChatRequest {
                    model: Some("body-model".to_string()),
                    ..Default::default()
                },
            )
            .expect_err("lookup should fail");

        assert!(matches!(result, Error::NoApiKey(_)));
    }

    #[test]
    fn options_model_is_alias() {
        let lookup = generate_lookup();
        let result = lookup
            .find_model_and_provider(
                &ProxyRequestOptions {
                    model: Some("alias-1".to_string()),
                    ..Default::default()
                },
                &ChatRequest {
                    model: Some("body-model".to_string()),
                    ..Default::default()
                },
            )
            .expect("should succeed");

        assert_eq!(result.alias, "alias-1");
        assert_eq!(result.random_order, false);
        assert_eq!(result.choices.len(), 2);

        assert_eq!(result.choices[0].model, "model-1");
        assert_eq!(result.choices[0].provider.name(), "openai");
        assert_eq!(
            result.choices[0].api_key,
            Some("actual-key-1-key".to_string())
        );

        assert_eq!(result.choices[1].model, "model-2");
        assert_eq!(result.choices[1].provider.name(), "anthropic");
        assert_eq!(result.choices[1].api_key, None);
    }

    #[test]
    fn body_model_is_alias() {
        let lookup = generate_lookup();
        let result = lookup
            .find_model_and_provider(
                &ProxyRequestOptions {
                    ..Default::default()
                },
                &ChatRequest {
                    model: Some("alias-2".to_string()),
                    ..Default::default()
                },
            )
            .expect("should succeed");

        assert_eq!(result.alias, "alias-2");
        assert_eq!(result.random_order, true);
        assert_eq!(result.choices.len(), 2);

        assert_eq!(result.choices[0].model, "model-1");
        assert_eq!(result.choices[0].provider.name(), "openai");
        assert_eq!(result.choices[0].api_key, None);

        assert_eq!(result.choices[1].model, "model-2");
        assert_eq!(result.choices[1].provider.name(), "anthropic");
        assert_eq!(
            result.choices[1].api_key,
            Some("actual-key-1-key".to_string())
        );
    }

    #[test]
    fn specified_provider() {
        let lookup = generate_lookup();
        let result = lookup
            .find_model_and_provider(
                &ProxyRequestOptions {
                    provider: Some("openai".to_string()),
                    ..Default::default()
                },
                &ChatRequest {
                    model: Some("a-model".to_string()),
                    ..Default::default()
                },
            )
            .expect("should succeed");

        assert_eq!(result.alias, "");
        assert_eq!(result.random_order, false);
        assert_eq!(result.choices.len(), 1);

        assert_eq!(result.choices[0].model, "a-model");
        assert_eq!(result.choices[0].provider.name(), "openai");
        assert_eq!(result.choices[0].api_key, None);
    }

    #[test]
    fn model_from_options() {
        let lookup = generate_lookup();
        let result = lookup
            .find_model_and_provider(
                &ProxyRequestOptions {
                    model: Some("override-model".to_string()),
                    provider: Some("openai".to_string()),
                    api_key: Some("a key".to_string()),
                    ..Default::default()
                },
                &ChatRequest {
                    model: Some("a-model".to_string()),
                    ..Default::default()
                },
            )
            .expect("should succeed");

        assert_eq!(result.alias, "");
        assert_eq!(result.random_order, false);
        assert_eq!(result.choices.len(), 1);

        assert_eq!(result.choices[0].model, "override-model");
        assert_eq!(result.choices[0].provider.name(), "openai");
        assert_eq!(result.choices[0].api_key, Some("a key".to_string()));
    }

    #[test]
    fn no_model() {
        let lookup = generate_lookup();
        let result = lookup
            .find_model_and_provider(
                &ProxyRequestOptions {
                    provider: Some("openai".to_string()),
                    api_key: Some("a key".to_string()),
                    ..Default::default()
                },
                &ChatRequest {
                    ..Default::default()
                },
            )
            .expect_err("should fail");

        assert!(matches!(result, Error::ModelNotSpecified));
    }

    #[test]
    fn alias_references_nonexistent_provider() {
        let lookup = generate_lookup();
        let result = lookup
            .find_model_and_provider(
                &ProxyRequestOptions {
                    model: Some("bad-provider-alias".to_string()),
                    ..Default::default()
                },
                &ChatRequest {
                    model: Some("body-model".to_string()),
                    ..Default::default()
                },
            )
            .expect_err("should fail");

        assert!(matches!(result, Error::NoAliasProvider(_, _)));
    }

    #[test]
    fn alias_references_nonexistent_apikey() {
        let lookup = generate_lookup();
        let result = lookup
            .find_model_and_provider(
                &ProxyRequestOptions {
                    model: Some("bad-key-alias".to_string()),
                    ..Default::default()
                },
                &ChatRequest {
                    model: Some("body-model".to_string()),
                    ..Default::default()
                },
            )
            .expect_err("should fail");

        assert!(matches!(result, Error::NoAliasApiKey(_, _)));
    }
}
