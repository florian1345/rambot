use serde::{Deserialize, Serialize};

use std::fs::{self, File};
use std::path::Path;

const DEFAULT_DEFAULT_GAUSSIAN_KERNEL_SIZE_SIGMAS: f32 = 5.0;
const DEFAULT_MAX_KERNEL_SIZE_SAMPLES: usize = 0;

#[derive(Clone, Deserialize, Serialize)]
pub(crate) struct Config {
    default_gaussian_kernel_size_sigmas: f32,
    max_kernel_size_samples: usize
}

impl Config {

    pub(crate) fn load(path: &str) -> Result<Config, String> {
        let path = Path::new(path);

        if path.is_dir() {
            Err("Config file is occupied by a directory.".to_owned())
        }
        else if path.is_file() {
            let json = fs::read_to_string(path)
                .map_err(|e| format!("Error reading config file: {}", e))?;
                
            Ok(serde_json::from_str(&json)
                .map_err(|e| format!("Error reading config file: {}", e))?)
        }
        else {
            let config = Config {
                default_gaussian_kernel_size_sigmas:
                    DEFAULT_DEFAULT_GAUSSIAN_KERNEL_SIZE_SIGMAS,
                max_kernel_size_samples: DEFAULT_MAX_KERNEL_SIZE_SAMPLES
            };
            let file = File::create(path)
                .map_err(|e| format!("Error creating config file: {}", e))?;
            serde_json::to_writer(file, &config)
                .map_err(|e| format!("Error writing config file: {}", e))?;

            Ok(config)
        }
    }

    pub(crate) fn default_gaussian_kernel_size_sigmas(&self) -> f32 {
        self.default_gaussian_kernel_size_sigmas
    }

    pub(crate) fn max_kernel_size_samples(&self) -> usize {
        self.max_kernel_size_samples
    }
}
