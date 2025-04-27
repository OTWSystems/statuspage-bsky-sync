use atrium_api::app::bsky::embed::external::ExternalData;
use atrium_api::app::bsky::feed::post::{RecordData, RecordEmbedRefs::AppBskyEmbedExternalMain};
use atrium_api::types::string::{Datetime, Language};
use bsky_sdk::BskyAgent;
use ipld_core::ipld::Ipld;
use lambda_http::{
    run, service_fn,
    tracing::{self, info},
    Body, Error, Request, RequestPayloadExt, Response,
};
use serde::Deserialize;
use time::OffsetDateTime;

#[derive(Clone, Deserialize)]
struct StatuspageEvent {
    incident: Option<StatuspageIncident>,
}

#[derive(Clone, Deserialize)]
struct StatuspageIncident {
    backfilled: bool,
    status: String,
    shortlink: String,
    incident_updates: Vec<StatuspageIncidentUpdate>,
    name: String,
}

#[derive(Clone, Deserialize)]
struct StatuspageIncidentUpdate {
    body: String,

    #[serde(with = "time::serde::iso8601")]
    display_at: OffsetDateTime,
}

fn to_bsky_post(incident_name: String, status: String, update: String, link: String) -> RecordData {
    let embed = Some(atrium_api::types::Union::Refs(AppBskyEmbedExternalMain(
        Box::new(atrium_api::types::Object {
            data: atrium_api::app::bsky::embed::external::MainData {
                external: atrium_api::types::Object {
                    data: ExternalData {
                        description: update.clone(),
                        thumb: None,
                        title: incident_name,
                        uri: link,
                    },
                    extra_data: Ipld::Null,
                },
            },
            extra_data: Ipld::Null,
        }),
    )));
    let langs = Some(vec![Language::new("en".into()).unwrap()]);
    RecordData {
        created_at: Datetime::now(),
        embed,
        entities: None,
        facets: None,
        labels: None,
        langs,
        reply: None,
        tags: None,
        text: format!("[update] {}: {}", status, update),
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
            let mut sorted_updates = incident.incident_updates.clone();
            sorted_updates.sort_by(|update1, update2| update2.display_at.cmp(&update1.display_at));

            let update_text: String = sorted_updates
                .first()
                .ok_or(Error::from("No incident update information provided"))?
                .body
                .chars()
                .take(250)
                .collect();

            let bsky_agent = BskyAgent::builder().build().await?;
            bsky_agent.login(bsky_username, bsky_password).await?;
            bsky_agent
                .create_record(to_bsky_post(
                    incident.name,
                    incident.status,
                    update_text,
                    incident.shortlink,
                ))
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
