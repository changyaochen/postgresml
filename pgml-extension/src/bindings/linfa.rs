use std::convert::From;

use linfa::prelude::Predict;
use linfa::traits::Fit;
use ndarray::{ArrayView1, ArrayView2};
use serde::{Deserialize, Serialize};

use super::Bindings;
use crate::orm::*;
use pgx::*;

#[derive(Debug, Serialize, Deserialize)]
pub struct LinearRegression {
    estimator: linfa_linear::FittedLinearRegression<f32>,
    num_features: usize,
}

impl LinearRegression {
    pub fn fit(dataset: &Dataset, hyperparams: &Hyperparams) -> Box<dyn Bindings>
    where
        Self: Sized,
    {
        let records = ArrayView2::from_shape(
            (dataset.num_train_rows, dataset.num_features),
            &dataset.x_train,
        )
        .unwrap();

        let targets = ArrayView1::from_shape(dataset.num_train_rows, &dataset.y_train).unwrap();

        let linfa_dataset = linfa::DatasetBase::from((records, targets));
        let mut estimator = linfa_linear::LinearRegression::default();

        for (key, value) in hyperparams {
            match key.as_str() {
                "fit_intercept" => {
                    estimator = estimator
                        .with_intercept(value.as_bool().expect("fit_intercept must be boolean"))
                }
                _ => error!("Unknown {}: {:?}", key.as_str(), value),
            };
        }

        let estimator = estimator.fit(&linfa_dataset).unwrap();

        Box::new(LinearRegression {
            estimator,
            num_features: dataset.num_features,
        })
    }
}

impl Bindings for LinearRegression {
    /// Predict a novel datapoint.
    fn predict(&self, features: &[f32]) -> f32 {
        self.predict_batch(features)[0]
    }

    /// Predict a novel datapoint.
    fn predict_batch(&self, features: &[f32]) -> Vec<f32> {
        let records = ArrayView2::from_shape(
            (features.len() / self.num_features, self.num_features),
            features,
        )
        .unwrap();
        self.estimator.predict(records).targets.into_raw_vec()
    }

    /// Serialize self to bytes
    fn to_bytes(&self) -> Vec<u8> {
        rmp_serde::to_vec(self).unwrap()
    }

    /// Deserialize self from bytes, with additional context
    fn from_bytes(bytes: &[u8]) -> Box<dyn Bindings>
    where
        Self: Sized,
    {
        let estimator: LinearRegression = rmp_serde::from_read(bytes).unwrap();
        Box::new(estimator)
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct LogisticRegression {
    estimator_binary: Option<linfa_logistic::FittedLogisticRegression<f32, i32>>,
    estimator_multi: Option<linfa_logistic::MultiFittedLogisticRegression<f32, i32>>,
    num_features: usize,
    num_distinct_labels: usize,
}

impl LogisticRegression {
    pub fn fit(dataset: &Dataset, hyperparams: &Hyperparams) -> Box<dyn Bindings>
    where
        Self: Sized,
    {
        let records = ArrayView2::from_shape(
            (dataset.num_train_rows, dataset.num_features),
            &dataset.x_train,
        )
        .unwrap();

        // Copy to convert to i32 because LogisticRegression doesn't contineous targets.
        let y_train: Vec<i32> = dataset.y_train.iter().map(|x| *x as i32).collect();
        let targets = ArrayView1::from_shape(dataset.num_train_rows, &y_train).unwrap();

        let linfa_dataset = linfa::DatasetBase::from((records, targets));

        if dataset.num_distinct_labels > 2 {
            let mut estimator = linfa_logistic::MultiLogisticRegression::default();

            for (key, value) in hyperparams {
                match key.as_str() {
                    "fit_intercept" => {
                        estimator = estimator
                            .with_intercept(value.as_bool().expect("fit_intercept must be boolean"))
                    }
                    "alpha" => {
                        estimator =
                            estimator.alpha(value.as_f64().expect("alpha must be a float") as f32)
                    }
                    "max_iterations" => {
                        estimator = estimator.max_iterations(
                            value.as_i64().expect("max_iterations must be an integer") as u64,
                        )
                    }
                    "gradient_tolerance" => {
                        estimator = estimator.gradient_tolerance(
                            value.as_f64().expect("gradient_tolerance must be a float") as f32,
                        )
                    }
                    _ => error!("Unknown {}: {:?}", key.as_str(), value),
                };
            }

            let estimator = estimator.fit(&linfa_dataset).unwrap();

            Box::new(LogisticRegression {
                estimator_binary: None,
                estimator_multi: Some(estimator),
                num_features: dataset.num_features,
                num_distinct_labels: dataset.num_distinct_labels,
            })
        } else {
            let mut estimator = linfa_logistic::LogisticRegression::default();

            for (key, value) in hyperparams {
                match key.as_str() {
                    "fit_intercept" => {
                        estimator = estimator
                            .with_intercept(value.as_bool().expect("fit_intercept must be boolean"))
                    }
                    "alpha" => {
                        estimator =
                            estimator.alpha(value.as_f64().expect("alpha must be a float") as f32)
                    }
                    "max_iterations" => {
                        estimator = estimator.max_iterations(
                            value.as_i64().expect("max_iterations must be an integer") as u64,
                        )
                    }
                    "gradient_tolerance" => {
                        estimator = estimator.gradient_tolerance(
                            value.as_f64().expect("gradient_tolerance must be a float") as f32,
                        )
                    }
                    _ => error!("Unknown {}: {:?}", key.as_str(), value),
                };
            }

            let estimator = estimator.fit(&linfa_dataset).unwrap();

            Box::new(LogisticRegression {
                estimator_binary: Some(estimator),
                estimator_multi: None,
                num_features: dataset.num_features,
                num_distinct_labels: dataset.num_distinct_labels,
            })
        }
    }
}

impl Bindings for LogisticRegression {
    /// Predict a novel datapoint.
    fn predict(&self, features: &[f32]) -> f32 {
        self.predict_batch(features)[0]
    }

    /// Predict a novel datapoint.
    fn predict_batch(&self, features: &[f32]) -> Vec<f32> {
        let records = ArrayView2::from_shape(
            (features.len() / self.num_features, self.num_features),
            features,
        )
        .unwrap();

        if self.num_distinct_labels > 2 {
            self.estimator_multi
                .as_ref()
                .unwrap()
                .predict(records)
                .targets
                .into_raw_vec()
                .into_iter()
                .map(|x| x as f32)
                .collect()
        } else {
            self.estimator_binary
                .as_ref()
                .unwrap()
                .predict(records)
                .targets
                .into_raw_vec()
                .into_iter()
                .map(|x| x as f32)
                .collect()
        }
    }

    /// Deserialize self from bytes, with additional context
    fn from_bytes(bytes: &[u8]) -> Box<dyn Bindings>
    where
        Self: Sized,
    {
        let estimator: LogisticRegression = rmp_serde::from_read(bytes).unwrap();
        Box::new(estimator)
    }

    /// Serialize self to bytes
    fn to_bytes(&self) -> Vec<u8> {
        rmp_serde::to_vec(self).unwrap()
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Svm {
    estimator: linfa_svm::Svm<f32, f32>,
    num_features: usize,
}

impl Svm {
    pub fn fit(dataset: &Dataset, hyperparams: &Hyperparams) -> Box<dyn Bindings> {
        let records = ArrayView2::from_shape(
            (dataset.num_train_rows, dataset.num_features),
            &dataset.x_train,
        )
        .unwrap();

        let targets = ArrayView1::from_shape(dataset.num_train_rows, &dataset.y_train).unwrap();

        let linfa_dataset = linfa::DatasetBase::from((records, targets));
        let mut estimator = linfa_svm::Svm::params();

        let mut hyperparams = hyperparams.clone();

        // Default to Guassian kernel, all the others are deathly slow.
        if !hyperparams.contains_key(&String::from("kernel")) {
            hyperparams.insert("kernel".to_string(), serde_json::Value::from("rbf"));
        }

        for (key, value) in hyperparams {
            match key.as_str() {
                "eps" => {
                    estimator = estimator.eps(value.as_f64().expect("eps must be a float") as f32)
                }
                "shrinking" => {
                    estimator =
                        estimator.shrinking(value.as_bool().expect("shrinking must be a bool"))
                }
                "kernel" => {
                    match value.as_str().expect("kernel must be a string") {
                        "poli" => estimator = estimator.polynomial_kernel(3.0, 1.0), // degree = 3, c = 1.0 as per Scikit
                        "linear" => estimator = estimator.linear_kernel(),
                        "rbf" => estimator = estimator.gaussian_kernel(1e-7), // Default eps
                        value => error!("Unknown kernel: {}", value),
                    }
                }
                _ => error!("Unknown {}: {:?}", key, value),
            }
        }

        let estimator = estimator.fit(&linfa_dataset).unwrap();

        Box::new(Svm {
            estimator,
            num_features: dataset.num_features,
        })
    }
}

impl Bindings for Svm {
    /// Predict a novel datapoint.
    fn predict(&self, features: &[f32]) -> f32 {
        self.predict_batch(features)[0]
    }

    /// Predict a novel datapoint.
    fn predict_batch(&self, features: &[f32]) -> Vec<f32> {
        let records = ArrayView2::from_shape(
            (features.len() / self.num_features, self.num_features),
            features,
        )
        .unwrap();

        self.estimator.predict(records).targets.into_raw_vec()
    }

    /// Deserialize self from bytes, with additional context
    fn from_bytes(bytes: &[u8]) -> Box<dyn Bindings>
    where
        Self: Sized,
    {
        let estimator: Svm = rmp_serde::from_read(bytes).unwrap();
        Box::new(estimator)
    }

    /// Serialize self to bytes
    fn to_bytes(&self) -> Vec<u8> {
        rmp_serde::to_vec(self).unwrap()
    }
}
