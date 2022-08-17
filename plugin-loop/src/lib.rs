use rambot_api::{
    AdapterResolver,
    AudioSourceList,
    ModifierDocumentation,
    ModifierDocumentationBuilder,
    Plugin,
    PluginConfig,
    PluginGuildConfig,
    ResolverRegistry
};

use std::collections::HashMap;
use std::io;

struct LoopAudioSourceList {
    child: Box<dyn AudioSourceList + Send + Sync>,
    buf: Vec<String>,
    idx: usize
}

impl AudioSourceList for LoopAudioSourceList {
    fn next(&mut self) -> Result<Option<String>, io::Error> {
        if let Some(s) = self.child.next()? {
            self.buf.push(s.clone());
            Ok(Some(s))
        }
        else if self.buf.is_empty() {
            Ok(None)
        }
        else {
            let result = self.buf[self.idx].clone();
            self.idx = (self.idx + 1) % self.buf.len();
            Ok(Some(result))
        }
    }
}

struct LoopAdapterResolver;

impl AdapterResolver for LoopAdapterResolver {
    fn name(&self) -> &str {
        "loop"
    }

    fn unique(&self) -> bool {
        true
    }

    fn documentation(&self) -> ModifierDocumentation {
        ModifierDocumentationBuilder::new()
            .with_short_summary(
                "Loops a playlist or single piece indefinitely.")
            .build().unwrap()
    }

    fn resolve(&self, _key_values: &HashMap<String, String>,
            child: Box<dyn AudioSourceList + Send + Sync>,
            _guild_config: PluginGuildConfig)
            -> Result<Box<dyn AudioSourceList + Send + Sync>, String> {
        Ok(Box::new(LoopAudioSourceList {
            child,
            buf: Vec::new(),
            idx: 0
        }))
    }
}

struct LoopPlugin;

impl Plugin for LoopPlugin {

    fn load_plugin<'registry>(&self, _config: PluginConfig,
            registry: &mut ResolverRegistry<'registry>) -> Result<(), String> {
        registry.register_adapter_resolver(LoopAdapterResolver);
        Ok(())
    }
}

fn make_loop_plugin() -> LoopPlugin {
    LoopPlugin
}

rambot_api::export_plugin!(make_loop_plugin);

#[cfg(test)]
mod tests {

    use super::*;

    use rambot_test_util::MockAudioSourceList;

    fn make_loop(child: MockAudioSourceList) -> Box<dyn AudioSourceList + Send + Sync> {
        let guild_config = PluginGuildConfig::default();
        let child = Box::new(child);

        LoopAdapterResolver.resolve(&HashMap::new(), child, guild_config)
            .unwrap()
    }

    const COLLECT_MAX_LEN: usize = 128;

    fn collect_loop(entries: Vec<&str>) -> Vec<String> {
        let child = MockAudioSourceList::new(entries);
        let mut looped = make_loop(child);

        rambot_test_util::collect_list(&mut looped, COLLECT_MAX_LEN).unwrap()
    }

    #[test]
    fn empty() {
        assert!(collect_loop(vec![]).is_empty());
    }

    #[test]
    fn singleton() {
        let collected = collect_loop(vec!["apple"]);

        assert_eq!(COLLECT_MAX_LEN, collected.len());

        for entry in collected {
            assert_eq!("apple", entry);
        }
    }

    #[test]
    fn three_entries() {
        let entries = vec!["apple", "banana", "cherry"];
        let collected = collect_loop(entries.clone());

        assert_eq!(COLLECT_MAX_LEN, collected.len());

        for (i, entry) in collected.iter().enumerate() {
            assert_eq!(entries[i % entries.len()], entry);
        }
    }
}
