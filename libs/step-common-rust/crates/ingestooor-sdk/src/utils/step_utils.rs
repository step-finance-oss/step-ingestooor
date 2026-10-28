use std::borrow::Borrow;

use reqwest::Url;

#[derive(Debug, Default)]
pub struct StepUtils {
    pub ipfs_url: Option<String>,
    pub json_metadata_http_client: reqwest::Client,
}

impl StepUtils {
    pub fn new(ipfs_url: Option<String>) -> Self {
        let json_metadata_http_client = reqwest::Client::new();
        StepUtils {
            ipfs_url,
            json_metadata_http_client,
        }
    }

    pub fn transform_ipfs_url(&self, metadata_uri: &str) -> Option<String> {
        let ipfs_gateway = self.ipfs_url.as_ref()?;
        if ipfs_gateway.is_empty() {
            return None;
        }

        let url = Url::parse(metadata_uri).ok()?;
        let get_cid_from_path = || {
            let segments = url.path_segments()?;
            let mut cid_segments = vec![];
            let mut found_valid_cid = false;
            for segment in segments {
                if found_valid_cid {
                    // Push the rest of the segments after the first valid CID
                    // To capture `/CID/378123.json`
                    cid_segments.push(segment);
                    continue;
                }

                if cid::Cid::try_from(segment).is_ok() {
                    cid_segments.push(segment);
                    found_valid_cid = true;
                }
            }

            if cid_segments.is_empty() {
                return None;
            }

            Some(cid_segments.join("/"))
        };
        let mut got_from_host = false;
        let get_cid_from_host = || {
            url.host()?
                .to_string()
                .split('.')
                .find(|domain| cid::Cid::try_from(*domain).is_ok())
                .map(|s| s.to_string())
        };
        let get_cid_from_query = || {
            url.query_pairs().find_map(|(_, v)| {
                let inner_str: &str = v.borrow();
                let cid_parsed = cid::Cid::try_from(inner_str).ok()?;
                Some(cid_parsed.to_string())
            })
        };

        let cid = get_cid_from_path()
            .or_else(|| {
                let host_cid = get_cid_from_host()?;
                got_from_host = true;
                Some(host_cid)
            })
            .or_else(get_cid_from_query)?;

        let base_new_url_string = if ipfs_gateway.contains("{}") {
            // Replace placeholder `{}` with the CID
            ipfs_gateway.replace("{}", &cid)
        } else {
            // Append `/ipfs/<CID>` to the base IPFS gateway
            format!("{}/ipfs/{}", ipfs_gateway.trim_end_matches('/'), cid)
        };

        let mut transformed_url = Url::parse(&base_new_url_string).ok()?;
        if got_from_host {
            if let Some(original_segments) = url.path_segments() {
                // Capture `/378123.json` (usually from nftstorage links)
                transformed_url
                    .path_segments_mut()
                    .ok()?
                    .extend(original_segments);
            }
        }

        if log::log_enabled!(log::Level::Debug) {
            // Parse the transformed URL and trim query parameters
            let parsed_url = transformed_url.clone();
            let trimmed_url = parsed_url.join("").ok()?.to_string();

            log::debug!(
                "Transformed metadata uri {} to {}",
                metadata_uri,
                trimmed_url
            );
        }

        Some(transformed_url.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn non_ipfs_urls_1() {
        let ipfs_url = Some(String::from("https://custom.gateway"));

        let step_utils = StepUtils::new(ipfs_url);

        let uri = "https://cdn.dexscreener.com/cms/tokens/metadata/UBPz3jzSGnZ3DK6tcczM";

        let transformed_uri = step_utils.transform_ipfs_url(uri);

        assert_eq!(
            transformed_uri, None,
            "The transformed URI should be None for non-IPFS URLs"
        );
    }

    #[test]
    fn non_ipfs_urls_2() {
        let ipfs_url = Some(String::from("https://custom.gateway"));

        let step_utils = StepUtils::new(ipfs_url);

        let uri = "https://position-nft.orca.so/meta/4VG8VFNo3EXiTA75mhh48qv55S6m3VA2uTQCzEt8kczb/7WePX2uttQxzMajxCoHvVH9w5dhhLv1jwEbkWSjdUgMa";

        let transformed_uri = step_utils.transform_ipfs_url(uri);

        assert_eq!(
            transformed_uri, None,
            "The transformed URI should be None for non-IPFS URLs"
        );
    }

    #[test]
    fn non_ipfs_urls_3() {
        let ipfs_url = Some(String::from("https://custom.gateway"));

        let step_utils = StepUtils::new(ipfs_url);

        let uri = "https://dynamic-ipfs.raydium.io/clmm/position?id=8BiFZsVZpzcJQu59JzCvwY8CEgqjcRtcKMviememzm6s";

        let transformed_uri = step_utils.transform_ipfs_url(uri);

        assert_eq!(
            transformed_uri, None,
            "The transformed URI should be None for non-IPFS URLs"
        );
    }

    #[test]
    fn non_ipfs_urls_4() {
        let ipfs_url = Some(String::from("https://custom.gateway"));

        let step_utils = StepUtils::new(ipfs_url);

        let uri = "https://arweave.net/NxlE1xnMA92QQIgIbWig3mPjEDhjCd8l6pCXnqyEPcg";

        let transformed_uri = step_utils.transform_ipfs_url(uri);

        assert_eq!(
            transformed_uri, None,
            "The transformed URI should be None for non-IPFS URLs"
        );
    }

    #[test]
    fn test_ipfs_url() {
        let ipfs_url = Some(String::from("https://custom.gateway"));

        let step_utils = StepUtils::new(ipfs_url);

        let uri = "https://example.com/ipfs/QmcqGtzqF8e7XThGfkGw9oxTedUMi5GqghsHJXKmCeBXQq";

        let transformed_uri = step_utils.transform_ipfs_url(uri);

        assert_eq!(
            transformed_uri,
            Some(
                "https://custom.gateway/ipfs/QmcqGtzqF8e7XThGfkGw9oxTedUMi5GqghsHJXKmCeBXQq"
                    .to_string()
            ),
            "The transformed URI should use the custom gateway with the CID"
        );
    }

    #[test]
    fn ipfs_url_with_cid_in_params() {
        let ipfs_url = Some(String::from("https://custom.gateway"));

        let step_utils = StepUtils::new(ipfs_url);

        let uri =
            "https://example.com/ipfs/get_object?cid=QmcqGtzqF8e7XThGfkGw9oxTedUMi5GqghsHJXKmCeBXQq";

        let transformed_uri = step_utils.transform_ipfs_url(uri);

        assert_eq!(
            transformed_uri,
            Some(
                "https://custom.gateway/ipfs/QmcqGtzqF8e7XThGfkGw9oxTedUMi5GqghsHJXKmCeBXQq"
                    .to_string()
            ),
            "The transformed URI should use the custom gateway with the CID from query params"
        );
    }

    #[test]
    fn test_ipfs_url_with_storage_link() {
        let ipfs_url = Some(String::from("https://custom.gateway"));

        let step_utils = StepUtils::new(ipfs_url);

        let uri = "https://bafkreihmjaybytksmpoam66j5web2sriajo2ostu3r2ybxlh4g3n2ghsse.ipfs.nftstorage.link";

        let transformed_uri = step_utils.transform_ipfs_url(uri);

        assert_eq!(
            transformed_uri,
            Some(
                "https://custom.gateway/ipfs/bafkreihmjaybytksmpoam66j5web2sriajo2ostu3r2ybxlh4g3n2ghsse/"
                    .to_string()
            ),
            "The transformed URI should use the custom gateway with the CID from storage link"
        );
    }

    #[test]
    fn test_ipfs_url_with_storage_link_and_path_to_json() {
        let ipfs_url = Some(String::from("https://custom.gateway"));

        let step_utils = StepUtils::new(ipfs_url);

        let uri = "https://bafkreihmjaybytksmpoam66j5web2sriajo2ostu3r2ybxlh4g3n2ghsse.ipfs.nftstorage.link/69.json";

        let transformed_uri = step_utils.transform_ipfs_url(uri);

        assert_eq!(
            transformed_uri,
            Some(
                "https://custom.gateway/ipfs/bafkreihmjaybytksmpoam66j5web2sriajo2ostu3r2ybxlh4g3n2ghsse/69.json"
                    .to_string()
            ),
            "The transformed URI should use the custom gateway with the CID from storage link"
        );
    }

    #[test]
    fn test_ipfs_url_with_storage_link_and_path_to_json_and_params() {
        let ipfs_url = Some(String::from("https://custom.gateway/ipfs/{}?token=1234"));

        let step_utils = StepUtils::new(ipfs_url);

        let uri = "https://bafkreihmjaybytksmpoam66j5web2sriajo2ostu3r2ybxlh4g3n2ghsse.ipfs.nftstorage.link/69.json";

        let transformed_uri = step_utils.transform_ipfs_url(uri);

        assert_eq!(
            transformed_uri,
            Some(
                "https://custom.gateway/ipfs/bafkreihmjaybytksmpoam66j5web2sriajo2ostu3r2ybxlh4g3n2ghsse/69.json?token=1234"
                    .to_string()
            ),
            "The transformed URI should use the custom gateway with the CID from storage link"
        );
    }

    #[test]
    fn test_ipfs_url_with_placeholder_in_gateway() {
        let ipfs_url = Some(String::from("https://custom.gateway/ipfs/{}?arg=1"));

        let step_utils = StepUtils::new(ipfs_url);

        let uri = "https://example.com/ipfs/QmcqGtzqF8e7XThGfkGw9oxTedUMi5GqghsHJXKmCeBXQq";

        let transformed_uri = step_utils.transform_ipfs_url(uri);

        assert_eq!(
            transformed_uri,
            Some(
                "https://custom.gateway/ipfs/QmcqGtzqF8e7XThGfkGw9oxTedUMi5GqghsHJXKmCeBXQq?arg=1"
                    .to_string()
            ),
            "The transformed URI should replace the placeholder with the CID"
        );
    }
}
