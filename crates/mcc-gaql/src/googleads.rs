use anyhow::{Result, bail};
use polars::prelude::*;
use tokio::io::AsyncWriteExt;
use tokio_stream::StreamExt;
use tonic::{
    Response, Streaming,
    codegen::InterceptedService,
    Status,
    transport::Channel,
};

use googleads_rs::google::ads::googleads::v23::services::{
    google_ads_service_client::GoogleAdsServiceClient,
    SearchGoogleAdsStreamRequest,
    SearchGoogleAdsStreamResponse,
};

use mcc_gaql_common::googleads_api::GoogleAdsAPIAccess;


// incomplete. Only what I need for the moment.
const GOOGLE_ADS_METRICS_INTEGER_FIELDS: &[&str] = &[
    "clicks",
    "cost_micros",
    "engagements",
    "historical_creative_quality_score",
    "historical_quality_score",
    "impressions",
    "interactions",
    "invalid_clicks",
    "organic_clicks",
    "organic_impressions",
    "organic_queries",
    "video_views",
    "view_through_conversions",
];

/// Run query via GoogleAdsServiceClient to get performance data
pub async fn gaql_query_with_client(
    mut client: GoogleAdsServiceClient<InterceptedService<Channel, GoogleAdsAPIAccess>>,
    customer_id: String,
    query: String,
) -> Result<(DataFrame, i64)> {
    let result: Result<Response<Streaming<SearchGoogleAdsStreamResponse>>, Status> = client
        .search_stream(SearchGoogleAdsStreamRequest {
            customer_id: customer_id.clone(),
            query,
            summary_row_setting: 0,
        })
        .await;

    let (df, total_api_consumption) = match result {
        Ok(response) => {
            let mut stream = response.into_inner();

            let mut columns: Vec<Vec<String>> = Vec::new();
            let mut headers: Option<Vec<String>> = None;
            let mut api_consumption: i64 = 0;

            while let Some(item) = stream.next().await {
                match item {
                    Ok(stream_response) => {
                        api_consumption += stream_response.query_resource_consumption;

                        let field_mask = stream_response.field_mask.unwrap();
                        if headers.is_none() {
                            headers = Some(field_mask.paths.clone());
                        }
                        for r in stream_response.results {
                            let row: googleads_rs::google::ads::googleads::v23::services::GoogleAdsRow = r;

                            for i in 0..headers.as_ref().unwrap().len() {
                                let path = &headers.as_ref().unwrap()[i];
                                let string_val: String =
                                    row.get(path).trim_matches('"').to_string();
                                match columns.get_mut(i) {
                                    Some(v) => {
                                        v.push(string_val);
                                    }
                                    None => {
                                        let v: Vec<String> = vec![string_val];
                                        columns.insert(i, v);
                                    }
                                }
                            }
                        }
                    }
                    Err(status) => {
                        let error_details = String::from_utf8_lossy(status.details())
                            .trim()
                            .replace(|c: char| !c.is_ascii(), "")
                            .replace("%", " ")
                            .replace("\n", " ")
                            .replace("\r", " ");

                        bail!(
                            "GoogleAdsClient streaming error. Account: {customer_id}, Message: '{}', Details: '{}'",
                            status.message(),
                            error_details
                        );
                    }
                }
            }

            let mut series_vec: Vec<Series> = Vec::new();

            if let Some(headers_vec) = headers {
                for (i, header) in headers_vec.iter().enumerate() {
                    if header.starts_with("metrics") {
                        if GOOGLE_ADS_METRICS_INTEGER_FIELDS
                            .iter()
                            .any(|f| f == header)
                        {
                            let v: Vec<Option<u64>> = columns
                                .get(i)
                                .map(|col| col.iter().map(|x| x.parse::<u64>().ok()).collect())
                                .unwrap_or_default();
                            series_vec.push(Series::new(header, v));
                        } else {
                            let v: Vec<Option<f64>> = columns
                                .get(i)
                                .map(|col| col.iter().map(|x| x.parse::<f64>().ok()).collect())
                                .unwrap_or_default();
                            series_vec.push(Series::new(header, v));
                        }
                    } else {
                        let v: Vec<String> = columns.get(i).cloned().unwrap_or_default();
                        series_vec.push(Series::new(header, v));
                    };
                }
            }

            let df = DataFrame::new(series_vec).unwrap();

            (df, api_consumption)

        }
        Err(status) => {
            bail!(
                "GoogleAdsClient request error. Account: {customer_id}, Message: {}, Details: {}",
                status.message(),
                String::from_utf8_lossy(status.details()).into_owned()
            );
        }
    };

    Ok((df, total_api_consumption))
}

/// Run query via GoogleAdsServiceClient to get performance data
pub async fn gaql_query(
    api_context: GoogleAdsAPIAccess,
    customer_id: String,
    query: String,
) -> Result<(DataFrame, i64)> {
    let client: GoogleAdsServiceClient<InterceptedService<Channel, GoogleAdsAPIAccess>> =
        GoogleAdsServiceClient::with_interceptor(api_context.channel.clone(), api_context);

    gaql_query_with_client(client, customer_id, query).await
}

/// Run query via GoogleAdsFieldService to obtain field metadata
pub async fn fields_query(api_context: GoogleAdsAPIAccess, query: &str) {
    let mut client =
        googleads_rs::google::ads::googleads::v23::services::google_ads_field_service_client::GoogleAdsFieldServiceClient::with_interceptor(api_context.channel.clone(), api_context);

    let response: googleads_rs::google::ads::googleads::v23::services::SearchGoogleAdsFieldsResponse = client
        .search_google_ads_fields(googleads_rs::google::ads::googleads::v23::services::SearchGoogleAdsFieldsRequest {
            query: query.to_owned(),
            page_token: String::new(),
            page_size: 10000,
        })
        .await
        .unwrap()
        .into_inner();

    let mut stdout = tokio::io::stdout();
    for row in response.results {
        let val = format!(
            "{}\t{:?}\t{}\t{}\t{:?}\n",
            row.name.as_deref().unwrap_or(""),
            row.category(),
            row.selectable.unwrap_or(false),
            row.filterable.unwrap_or(false),
            row.selectable_with,
        );
        stdout.write_all(val.as_bytes()).await.unwrap();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_integer_metric_parsing_valid_values() {
        let input = ["100".to_string(), "200".to_string(), "0".to_string()];
        let result: Vec<Option<u64>> = input.iter().map(|x| x.parse::<u64>().ok()).collect();
        assert_eq!(result, vec![Some(100), Some(200), Some(0)]);
    }

    #[test]
    fn test_integer_metric_parsing_invalid_values() {
        let input = [
            "".to_string(),
            "--".to_string(),
            "N/A".to_string(),
            " ".to_string(),
        ];
        let result: Vec<Option<u64>> = input.iter().map(|x| x.parse::<u64>().ok()).collect();
        assert_eq!(result, vec![None, None, None, None]);
    }

    #[test]
    fn test_float_metric_parsing_valid_values() {
        let input = ["1.5".to_string(), "0.0".to_string(), "99.99".to_string()];
        let result: Vec<Option<f64>> = input.iter().map(|x| x.parse::<f64>().ok()).collect();
        assert_eq!(result, vec![Some(1.5), Some(0.0), Some(99.99)]);
    }

    #[test]
    fn test_series_from_optional_integer_values() {
        let values: Vec<Option<u64>> = vec![Some(100), None, Some(200), None, Some(50)];
        let series = Series::new("metrics.clicks", values);
        assert_eq!(series.len(), 5);
        assert_eq!(series.null_count(), 2);
    }
}
