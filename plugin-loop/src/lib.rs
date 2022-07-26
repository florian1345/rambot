use rambot_api::{
    AdapterResolver,
    AudioSourceList,
    Plugin,
    PluginConfig,
    ResolverRegistry
};

use std::collections::HashMap;
use std::io;

struct LoopAudioSourceList {
    child: Box<dyn AudioSourceList + Send>,
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

    fn resolve(&self, _key_values: &HashMap<String, String>,
            child: Box<dyn AudioSourceList + Send>)
            -> Result<Box<dyn AudioSourceList + Send>, String> {
        Ok(Box::new(LoopAudioSourceList {
            child,
            buf: Vec::new(),
            idx: 0
        }))
    }
}

struct LoopPlugin;

impl Plugin for LoopPlugin {

    fn load_plugin<'registry>(&mut self, _config: &PluginConfig,
            registry: &mut ResolverRegistry<'registry>) -> Result<(), String> {
        registry.register_adapter_resolver(LoopAdapterResolver);
        Ok(())
    }
}

fn make_loop_plugin() -> LoopPlugin {
    LoopPlugin
}

rambot_api::export_plugin!(make_loop_plugin);

