// Pool of LLM models with per-model concurrency control.
//
// Each model gets a semaphore with exactly 1 permit, ensuring that at most one
// request is in-flight per model at any time (matching provider rate-limit
// constraints).  Callers acquire a `ModelLease`, use it to create an agent,
// then simply drop the lease to release the slot.

use std::sync::Arc;

use rig::agent::Agent;
use rig::providers::openai::completion::CompletionModel;
use tokio::sync::{OwnedSemaphorePermit, Semaphore};

use crate::rag::LlmConfig;

/// Pool of LLM models with per-model concurrency control.
///
/// Each model has a semaphore with 1 permit, ensuring only one request per
/// model is in-flight at any time.  Use [`ModelPool::acquire`] to obtain the
/// next available model, or [`ModelPool::acquire_preferred`] to wait
/// specifically for model index 0.
pub struct ModelPool {
    config: Arc<LlmConfig>,
    /// One semaphore per model; semaphore index == model index in `config`.
    semaphores: Vec<Arc<Semaphore>>,
}

impl ModelPool {
    /// Create a new model pool from LLM configuration.
    ///
    /// One semaphore (1 permit) is allocated for every model in `config`.
    pub fn new(config: Arc<LlmConfig>) -> Self {
        let semaphores = config
            .all_models()
            .iter()
            .map(|_| Arc::new(Semaphore::new(1)))
            .collect();

        Self { config, semaphores }
    }

    /// Returns the number of models in the pool.
    pub fn model_count(&self) -> usize {
        self.semaphores.len()
    }

    /// Acquire any available model using a least-busy-first strategy.
    ///
    /// Models are tried in order (index 0 first) without blocking.  If all
    /// models are currently busy the method races all semaphores and returns
    /// whichever becomes free first.
    pub async fn acquire(&self) -> ModelLease {
        // Fast path: try each model in order without waiting.
        for (idx, sem) in self.semaphores.iter().enumerate() {
            if let Ok(permit) = sem.clone().try_acquire_owned() {
                log::debug!(
                    "Acquired model '{}' (index {}) immediately",
                    self.config.all_models()[idx],
                    idx
                );
                return ModelLease {
                    model_index: idx,
                    model_name: self.config.all_models()[idx].clone(),
                    config: Arc::clone(&self.config),
                    _permit: permit,
                };
            }
        }

        // Slow path: all models busy – race all semaphores and take the winner.
        log::debug!("All models busy, waiting for any model to become available...");

        let futures: Vec<_> = self
            .semaphores
            .iter()
            .enumerate()
            .map(|(idx, sem)| {
                let sem = Arc::clone(sem);
                Box::pin(async move {
                    let permit = sem
                        .acquire_owned()
                        .await
                        .expect("semaphore closed unexpectedly");
                    (idx, permit)
                })
            })
            .collect();

        let ((idx, permit), _, _) = futures::future::select_all(futures).await;

        log::debug!(
            "Acquired model '{}' (index {}) after waiting",
            self.config.all_models()[idx],
            idx
        );

        ModelLease {
            model_index: idx,
            model_name: self.config.all_models()[idx].clone(),
            config: Arc::clone(&self.config),
            _permit: permit,
        }
    }

    /// Acquire the preferred (first) model specifically.
    ///
    /// Blocks until model index 0 is available.  Use this for operations that
    /// should always use the primary model regardless of pool load.
    pub async fn acquire_preferred(&self) -> ModelLease {
        let permit = self.semaphores[0]
            .clone()
            .acquire_owned()
            .await
            .expect("semaphore closed unexpectedly");

        log::debug!(
            "Acquired preferred model '{}' (index 0)",
            self.config.all_models()[0]
        );

        ModelLease {
            model_index: 0,
            model_name: self.config.all_models()[0].clone(),
            config: Arc::clone(&self.config),
            _permit: permit,
        }
    }
}

/// A lease on a specific model.  The associated semaphore permit is released
/// when this value is dropped, making the model slot available again.
pub struct ModelLease {
    model_index: usize,
    model_name: String,
    config: Arc<LlmConfig>,
    _permit: OwnedSemaphorePermit,
}

impl ModelLease {
    /// Returns the model name for this lease.
    pub fn model_name(&self) -> &str {
        &self.model_name
    }

    /// Returns the model index (0 == preferred model).
    #[allow(dead_code)]
    pub fn model_index(&self) -> usize {
        self.model_index
    }

    /// Returns the temperature setting from the config.
    pub fn temperature(&self) -> f32 {
        self.config.temperature()
    }

    /// Create an LLM agent for this model with the given system prompt.
    pub fn create_agent(
        &self,
        system_prompt: &str,
    ) -> Result<Agent<CompletionModel>, anyhow::Error> {
        self.config
            .create_agent_for_model(&self.model_name, system_prompt)
    }
}

impl Drop for ModelLease {
    fn drop(&mut self) {
        log::debug!(
            "Released model '{}' (index {})",
            self.model_name,
            self.model_index
        );
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::rag::LlmConfig;

    /// Build a `ModelPool` with the given model names for testing purposes.
    fn make_pool(model_names: &[&str]) -> ModelPool {
        let models: Vec<String> = model_names.iter().map(|s| s.to_string()).collect();
        let config = Arc::new(LlmConfig::from_models_for_test(models));
        ModelPool::new(config)
    }

    #[test]
    fn test_model_pool_model_count() {
        let pool = make_pool(&["model-a", "model-b", "model-c"]);
        assert_eq!(pool.model_count(), 3);
    }

    #[tokio::test]
    async fn test_acquire_returns_available_model() {
        let pool = make_pool(&["model-a", "model-b", "model-c"]);
        let lease = pool.acquire().await;
        assert_eq!(lease.model_name(), "model-a");
        assert_eq!(lease.model_index(), 0);
    }

    #[tokio::test]
    async fn test_acquire_preferred_always_index_zero() {
        let pool = Arc::new(make_pool(&["model-a", "model-b"]));
        let lease = pool.acquire_preferred().await;
        assert_eq!(lease.model_index(), 0);
        assert_eq!(lease.model_name(), "model-a");
    }

    #[tokio::test]
    async fn test_acquire_uses_next_available_when_preferred_busy() {
        let pool = make_pool(&["model-a", "model-b", "model-c"]);
        // Hold model-a
        let _lease_a = pool.acquire().await;
        assert_eq!(_lease_a.model_index(), 0);

        // Next acquire should go to model-b (index 1)
        let lease_b = pool.acquire().await;
        assert_eq!(lease_b.model_index(), 1);
        assert_eq!(lease_b.model_name(), "model-b");
    }

    #[tokio::test]
    async fn test_acquire_three_concurrent_different_models() {
        let pool = Arc::new(make_pool(&["model-a", "model-b", "model-c"]));

        let lease0 = pool.acquire().await;
        let lease1 = pool.acquire().await;
        let lease2 = pool.acquire().await;

        let mut indices = [
            lease0.model_index(),
            lease1.model_index(),
            lease2.model_index(),
        ];
        indices.sort();
        assert_eq!(indices, [0, 1, 2]);
    }

    #[tokio::test]
    async fn test_acquire_waits_when_all_busy() {
        use tokio::time::{Duration, timeout};

        let pool = Arc::new(make_pool(&["model-a"]));

        let lease = pool.acquire().await;
        assert_eq!(lease.model_index(), 0);

        let pool_clone = Arc::clone(&pool);
        let result = timeout(Duration::from_millis(50), async move {
            pool_clone.acquire().await
        })
        .await;

        assert!(
            result.is_err(),
            "acquire should have timed out while all models are busy"
        );

        drop(lease);

        let lease2 = pool.acquire().await;
        assert_eq!(lease2.model_index(), 0);
    }

    #[tokio::test]
    async fn test_acquire_preferred_waits_and_releases() {
        use tokio::time::{Duration, timeout};

        let pool = Arc::new(make_pool(&["model-a"]));

        let lease = pool.acquire_preferred().await;
        assert_eq!(lease.model_index(), 0);

        let pool_clone = Arc::clone(&pool);
        let result = timeout(Duration::from_millis(50), async move {
            pool_clone.acquire_preferred().await
        })
        .await;

        assert!(result.is_err(), "acquire_preferred should have timed out");
        drop(lease);

        let lease2 = pool.acquire_preferred().await;
        assert_eq!(lease2.model_index(), 0);
    }
}
