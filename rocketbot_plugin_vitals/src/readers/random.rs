use async_trait::async_trait;
use rand::{Rng, SeedableRng};
use rand::rngs::StdRng;
use rocketbot_interface::JsonValueExtensions;
use rocketbot_interface::sync::Mutex;

use crate::interface::VitalsReader;


#[derive(Clone, Copy, Debug, PartialEq)]
enum SingleValue {
    Random(RandomValue),
    ConversionReference(ConversionValue),
}
impl SingleValue {
    pub fn decimals(&self) -> usize {
        match self {
            Self::Random(rv) => rv.decimals,
            Self::ConversionReference(cr) => cr.decimals,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct RandomValue {
    pub min_value: f64,
    pub max_value: f64,
    pub resolution: f64,
    pub decimals: usize,
}
impl RandomValue {
    pub fn new(
        min_value: f64,
        max_value: f64,
        resolution: f64,
        decimals: usize,
    ) -> Self {
        Self {
            min_value,
            max_value,
            resolution,
            decimals,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct ConversionValue {
    pub reference: usize,
    pub factor: f64,
    pub decimals: usize,
}
impl ConversionValue {
    pub fn new(
        reference: usize,
        factor: f64,
        decimals: usize,
    ) -> Self {
        Self {
            reference,
            factor,
            decimals,
        }
    }
}


pub(crate) struct RandomReader {
    values: Vec<SingleValue>,
    format_string: String,
    rng: Mutex<StdRng>,
}
#[async_trait]
impl VitalsReader for RandomReader {
    async fn new(config: &serde_json::Value) -> Self {
        let format_string = config["format_string"]
            .as_str().expect("format_string is not a string")
            .to_owned();

        let config_values = config["values"]
            .as_array().expect("values is not a string");
        let mut values = Vec::with_capacity(config_values.len());
        for config_value in config_values {
            if let Some(min_value) = config_value["min_value"].as_f64() {
                let max_value = config_value["max_value"]
                    .as_f64().expect("max_value not an f64");
                let resolution = config_value["resolution"]
                    .as_f64().expect("resolution not an f64");
                let decimals = config_value["decimals"]
                    .as_usize().expect("decimals not a usize");
                values.push(SingleValue::Random(RandomValue::new(min_value, max_value, resolution, decimals)));
            } else if let Some(reference) = config_value["conversion_reference"].as_usize() {
                let factor = config_value["conversion_factor"]
                    .as_f64().expect("conversion_factor not an f64");
                let decimals = config_value["decimals"]
                    .as_usize().expect("decimals not a usize");
                values.push(SingleValue::ConversionReference(ConversionValue::new(reference, factor, decimals)));
            } else {
                panic!("failed to deduce value type");
            }
        }

        let rng = Mutex::new(
            "RandomReader::rng",
            StdRng::from_entropy(),
        );

        Self {
            values,
            format_string,
            rng,
        }
    }

    async fn read(&self) -> Option<String> {
        let mut rng_guard = self.rng.lock().await;

        let mut final_values = vec![0.0; self.values.len()];
        for i in 0..final_values.len() {
            match &self.values[i] {
                SingleValue::ConversionReference(cv) => {
                    final_values[i] = final_values[cv.reference] * cv.factor;
                },
                SingleValue::Random(rv) => {
                    let span = ((rv.max_value - rv.min_value) / rv.resolution) as i64;
                    let add = rng_guard.gen_range(0..span);
                    final_values[i] = rv.min_value + (add as f64 * rv.resolution);
                },
            }
        }

        let final_strings: Vec<String> = self.values.iter().zip(final_values.iter())
            .map(|(v, fv)| format!("{:.*}", v.decimals(), fv))
            .collect();

        let mut output_string = self.format_string.clone();
        for (i, fs) in final_strings.iter().enumerate() {
            output_string = output_string.replace(&format!("{}{}{}", '{', i, '}'), fs);
        }
        Some(output_string)
    }
}
