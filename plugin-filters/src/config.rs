use rambot_api::SampleDuration;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use std::fs::{self, File};
use std::path::Path;

const DEFAULT_DEFAULT_GAUSSIAN_KERNEL_SIZE_SIGMAS: f32 = 5.0;
const DEFAULT_MAX_KERNEL_SIZE_SAMPLES: usize = 0;
const DEFAULT_MAX_ECHO_DELAY: SampleDuration =
    SampleDuration::from_samples(rambot_api::SAMPLES_PER_MINUTE);

fn serialize_duration<S>(x: &SampleDuration, s: S) -> Result<S::Ok, S::Error>
where
    S: Serializer
{
    s.serialize_i64(x.samples())
}

fn deserialize_duration<'a, D>(d: D) -> Result<SampleDuration, D::Error>
where
    D: Deserializer<'a>
{
    Ok(SampleDuration::from_samples(i64::deserialize(d)?))
}

#[derive(Clone, Deserialize, Serialize)]
pub(crate) struct Config {
    default_gaussian_kernel_size_sigmas: f32,
    max_kernel_size_samples: usize,

    #[serde(serialize_with = "serialize_duration")]
    #[serde(deserialize_with = "deserialize_duration")]
    max_echo_delay: SampleDuration
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
                max_kernel_size_samples: DEFAULT_MAX_KERNEL_SIZE_SAMPLES,
                max_echo_delay: DEFAULT_MAX_ECHO_DELAY
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

    pub(crate) fn max_echo_delay(&self) -> SampleDuration {
        self.max_echo_delay
    }
}
