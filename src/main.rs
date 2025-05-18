use atrium_api::app::bsky::embed::external::ExternalData;
use atrium_api::app::bsky::feed::post::{RecordData, RecordEmbedRefs::AppBskyEmbedExternalMain};
use atrium_api::types::string::{Datetime, Language};
use bsky_sdk::BskyAgent;
use convert_case::{Case, Casing};
use ipld_core::ipld::Ipld;
use lambda_http::{
    run, service_fn,
    tracing::{self, info},
    Body, Error, Request, RequestPayloadExt, Response,
};
use serde::Deserialize;
use serde_with::{serde_as, DisplayFromStr};

#[derive(Clone, Deserialize)]
struct StatuspageEvent {
    incident: Option<StatuspageIncident>,
}

#[derive(Clone, Deserialize)]
struct StatuspageIncident {
    #[serde(default = "bool::default")]
    backfilled: bool,
    status: String,
    shortlink: String,
    incident_updates: Vec<StatuspageIncidentUpdate>,
    name: String,
}

#[serde_as]
#[derive(Clone, Deserialize)]
struct StatuspageIncidentUpdate {
    #[serde_as(as = "_")]
    body: String,

    #[serde_as(as = "DisplayFromStr")]
    display_at: Datetime,
}

impl TryFrom<StatuspageIncident> for RecordData {
    type Error = Error;

    fn try_from(incident: StatuspageIncident) -> Result<RecordData, Self::Error> {
        let mut sorted_updates = incident.incident_updates.clone();
        sorted_updates.sort_by(|update1, update2| update2.display_at.cmp(&update1.display_at));

        let latest_update = sorted_updates
            .first()
            .ok_or(Error::from("No incident update information provided"))?;
        let mut update_text: String = latest_update.body.chars().take(250).collect();
        if latest_update.body.chars().count() > 250 {
            update_text = format!("{}...", update_text);
        }

        let embed = Some(atrium_api::types::Union::Refs(AppBskyEmbedExternalMain(
            Box::new(atrium_api::types::Object {
                data: atrium_api::app::bsky::embed::external::MainData {
                    external: atrium_api::types::Object {
                        data: ExternalData {
                            description: update_text.clone(),
                            thumb: None,
                            title: incident.name,
                            uri: incident.shortlink,
                        },
                        extra_data: Ipld::Null,
                    },
                },
                extra_data: Ipld::Null,
            }),
        )));
        let langs = Some(vec![Language::new("en".into()).unwrap()]);
        Ok(RecordData {
            created_at: latest_update.display_at.clone(),
            embed,
            entities: None,
            facets: None,
            labels: None,
            langs,
            reply: None,
            tags: None,
            text: format!(
                "[update] {}: {}",
                incident.status.to_case(Case::Title),
                update_text
            ),
        })
    }
}

async fn function_handler(event: Request) -> Result<Response<Body>, Error> {
    let bsky_username = std::env::var("BSKY_USERNAME")?;
    let bsky_password = std::env::var("BSKY_PASSWORD")?;

    let statuspage_event = event
        .payload::<StatuspageEvent>()?
        .ok_or(Error::from("No payload provided"))?;

    if let Some(incident) = statuspage_event.incident {
        if incident.backfilled {
            info!("skipping backfilled incident");
        } else {
            let bsky_agent = BskyAgent::builder().build().await?;
            bsky_agent.login(bsky_username, bsky_password).await?;
            bsky_agent
                .create_record(RecordData::try_from(incident)?)
                .await?;
        }
    } else {
        info!("skipping statuspage event with no incident details");
    }

    let resp = Response::builder()
        .status(200)
        .header("content-type", "text/html")
        .body("Success".into())
        .map_err(Box::new)?;
    Ok(resp)
}

#[tokio::main]
async fn main() -> Result<(), Error> {
    // required to enable CloudWatch error logging by the runtime
    tracing::init_default_subscriber();

    run(service_fn(function_handler)).await
}
