use std::{collections::HashMap, sync::Arc};

use color_eyre::eyre;
use futures::stream::{StreamExt, TryStreamExt};

use super::{
    config,
    notifiers::{self, DynNotifier},
};

pub struct Rule {
    pub rule: config::EventFilter,
    pub destination: Vec<RuleDestination>,
}

#[derive(Clone)]
pub struct RuleDestination {
    pub template: Arc<liquid::Template>,
    pub notifier: Arc<DynNotifier>,
}

impl Rule {
    pub async fn from_config(config: config::Config) -> eyre::Result<Vec<Self>> {
        // Instantiate all the notifiers in boxes
        let notifiers = futures::stream::iter(config.notifiers)
            .then(|config| async move {
                let notifier = match config.spec {
                    config::NotifierSpec::Matrix {
                        homeserver_url,
                        user_id,
                        password_env,
                        room_id,
                    } => Arc::new(Box::new(
                        notifiers::Matrix::new(
                            homeserver_url,
                            user_id,
                            &std::env::var(password_env)?,
                            room_id,
                        )
                        .await?,
                    ) as DynNotifier),
                };

                let template = Arc::new(liquid::Parser::new().parse(&config.template)?);

                Ok::<_, eyre::Report>((config.name, RuleDestination { notifier, template }))
            })
            .try_collect::<HashMap<_, _>>()
            .await?;

        config
            .events
            .into_iter()
            .map(|filter| {
                let notifiers = filter
                    .to
                    .iter()
                    .map(|name| {
                        notifiers
                            .get(name)
                            .ok_or_else(|| {
                                eyre::eyre!(
                                    "Notifier named '{name}' was not defined in the configuration",
                                )
                            })
                            .map(Clone::clone)
                    })
                    .collect::<eyre::Result<Vec<_>>>()?;

                Ok(Self {
                    rule: filter,
                    destination: notifiers,
                })
            })
            .collect()
    }
}