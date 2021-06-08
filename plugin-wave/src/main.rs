use rambot_api::plugin::{PluginAppBuilder, PluginBuilder};

#[tokio::main]
async fn main() {
    let errors = PluginAppBuilder::new()
        .with_plugin(PluginBuilder::new()
            .build())
        .build().launch().await;

    for e in errors {
        eprintln!("Error in plugin: {}", e);
    }
}
