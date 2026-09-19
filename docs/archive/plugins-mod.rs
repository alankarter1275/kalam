// Scaffolding for Phase 4 (Wasm Plugins). These structs are intentionally unused right now.
#![allow(dead_code)]


use std::collections::HashMap;
use wasmtime::{Config, Engine, Store, component::{Component, Linker}};
use anyhow::Result;


wasmtime::component::bindgen!({
    path: "wit/kalam.wit",
    world: "plugin",
    async: false,
});

pub struct WasmState {
    pub engine: Engine,
}

impl kalam::plugin::host::Host for WasmState {
    fn fetch(&mut self, url: String, headers: Vec<(String, String)>) -> Result<Vec<u8>, String> {
        let mut req = ureq::get(&url);
        for (k, v) in headers {
            req = req.set(&k, &v);
        }
        match req.call() {
            Ok(resp) => {
                let mut buf = Vec::new();
                if let Err(e) = std::io::Read::read_to_end(&mut resp.into_reader(), &mut buf) {
                    return Err(e.to_string());
                }
                Ok(buf)
            }
            Err(e) => Err(e.to_string()),
        }
    }
}

pub struct PluginSystem {
    pub engine: Engine,
    pub linker: Linker<WasmState>,
}

impl PluginSystem {
    pub fn new() -> Result<Self> {
        let mut config = Config::new();
        config.wasm_component_model(true);
        config.async_support(false);

        let engine = Engine::new(&config)?;
        let mut linker = Linker::new(&engine);
        kalam::plugin::host::add_to_linker(&mut linker, |state: &mut WasmState| state)?;

        Ok(Self { engine, linker })
    }

    pub fn load_scraper(&self, path: &std::path::Path) -> Result<WasmSource> {
        let component = Component::from_file(&self.engine, path)?;
        
        // We do a test instantiation to extract the id and name
        let mut store = Store::new(&self.engine, WasmState { engine: self.engine.clone() });
        let plugin = Plugin::instantiate(&mut store, &component, &self.linker)?;
        let scraper = plugin.kalam_plugin_scraper();
        
        let id = scraper.call_id(&mut store)?;
        let name = scraper.call_name(&mut store)?;
        let base_url = scraper.call_base_url(&mut store)?;

        Ok(WasmSource {
            engine: self.engine.clone(),
            linker: self.linker.clone(),
            component,
            id,
            name,
            base_url,
        })
    }
}

pub struct WasmSource {
    engine: Engine,
    linker: Linker<WasmState>,
    component: Component,
    id: String,
    name: String,
    base_url: String,
}

impl WasmSource {
    fn instantiate(&self) -> Result<(Store<WasmState>, Plugin)> {
        let mut store = Store::new(&self.engine, WasmState { engine: self.engine.clone() });
        let plugin = Plugin::instantiate(&mut store, &self.component, &self.linker)?;
        Ok((store, plugin))
    }
}

impl crate::sources::Source for WasmSource {
    fn id(&self) -> &str { &self.id }
    fn name(&self) -> &str { &self.name }
    fn base_url(&self) -> &str { &self.base_url }

    fn get_filter_definitions(&self) -> Vec<crate::sources::FilterDefinition> {
        if let Ok((mut store, plugin)) = self.instantiate() {
            if let Ok(defs) = plugin.kalam_plugin_scraper().call_get_filter_definitions(&mut store) {
                return defs.into_iter().map(|d| {
                    crate::sources::FilterDefinition {
                        id: d.id,
                        name: d.name,
                        filter_type: match d.filter_type {
                            kalam::plugin::types::FilterType::Text(p) => crate::sources::FilterType::Text { placeholder: p },
                            kalam::plugin::types::FilterType::Checkbox => crate::sources::FilterType::Checkbox,
                            kalam::plugin::types::FilterType::Select(opts) => crate::sources::FilterType::Select { options: opts },
                            kalam::plugin::types::FilterType::Sort(opts) => crate::sources::FilterType::Sort { options: opts },
                        },
                        default_value: d.default_value,
                    }
                }).collect();
            }
        }
        Vec::new()
    }

    fn search(&self, query: &str, page: u32, filters: &HashMap<String, String>) -> Result<crate::sources::SearchPage> {
        let (mut store, plugin) = self.instantiate()?;
        let filter_list: Vec<(String, String)> = filters.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
        let res = plugin.kalam_plugin_scraper().call_search(&mut store, query, page, &filter_list)?;
        let res = res.map_err(|e| anyhow::anyhow!(e))?;

        Ok(crate::sources::SearchPage {
            results: res.results.into_iter().map(|r| crate::sources::RemoteBookCard {
                remote_id: r.remote_id,
                title: r.title,
                author: r.author,
                cover_url: r.cover_url,
            }).collect(),
            has_more: res.has_more,
        })
    }

    fn get_details(&self, remote_id: &str) -> Result<crate::sources::RemoteBookDetails> {
        let (mut store, plugin) = self.instantiate()?;
        let res = plugin.kalam_plugin_scraper().call_get_details(&mut store, remote_id)?;
        let res = res.map_err(|e| anyhow::anyhow!(e))?;

        Ok(crate::sources::RemoteBookDetails {
            remote_id: res.remote_id,
            title: res.title,
            author: res.author,
            description: res.description,
            cover_url: res.cover_url,
            tags: res.tags,
            status: res.status,
        })
    }

    fn get_chapters(&self, remote_id: &str) -> Result<Vec<crate::sources::RemoteChapter>> {
        let (mut store, plugin) = self.instantiate()?;
        let res = plugin.kalam_plugin_scraper().call_get_chapters(&mut store, remote_id)?;
        let res = res.map_err(|e| anyhow::anyhow!(e))?;

        Ok(res.into_iter().map(|c| crate::sources::RemoteChapter {
            chapter_id: c.chapter_id,
            title: c.title,
            number: c.number,
            volume: c.volume,
            url: c.url,
        }).collect())
    }

    fn get_chapter_content(&self, chapter_id: &str) -> Result<crate::sources::ChapterContent> {
        let (mut store, plugin) = self.instantiate()?;
        let res = plugin.kalam_plugin_scraper().call_get_chapter_content(&mut store, chapter_id)?;
        let res = res.map_err(|e| anyhow::anyhow!(e))?;

        match res {
            kalam::plugin::types::ChapterContent::Images(imgs) => Ok(crate::sources::ChapterContent::Images(imgs)),
            kalam::plugin::types::ChapterContent::Html(html) => Ok(crate::sources::ChapterContent::Html(html)),
        }
    }

    fn fetch_image(&self, url: &str) -> Result<Vec<u8>> {
        // Just use standard fetch for images for now, this could be delegated to Wasm later if needed.
        let req = ureq::get(url);
        // We could extract host logic here.
        let resp = req.call()?;
        let mut buf = Vec::new();
        std::io::Read::read_to_end(&mut resp.into_reader(), &mut buf)?;
        Ok(buf)
    }
}
