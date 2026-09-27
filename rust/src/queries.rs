//! GraphQL query text, byte-for-byte identical to the Python implementation's
//! (`src/queries.py`, after `str.format` has collapsed the doubled braces).
//! Generated from src/queries.py - do not reformat.

/// Replace `{name}` placeholders with the supplied values.
pub fn fill(template: &str, args: &[(&str, &str)]) -> String {
    let mut out = String::with_capacity(template.len() + 64);
    let mut rest = template;
    while let Some(start) = rest.find('{') {
        let after = &rest[start + 1..];
        if let Some(end) = after.find('}') {
            let name = &after[..end];
            if let Some((_, value)) = args.iter().find(|(key, _)| *key == name) {
                out.push_str(&rest[..start]);
                out.push_str(value);
                rest = &after[end + 1..];
                continue;
            }
        }
        out.push_str(&rest[..start + 1]);
        rest = &rest[start + 1..];
    }
    out.push_str(rest);
    out
}

#[allow(dead_code)]
pub const TOKEN_QUERY: &str = r#"mutation {
	obtainKrakenToken(input: { APIKey: "{api_key}" }) {
	    token
	}
}"#;

#[allow(dead_code)]
pub const ACCEPT_TERMS_QUERY: &str = r#"mutation {
    acceptTermsAndConditions(input: {
        accountNumber: "{account_number}",
        enrolmentId: "{enrolment_id}",
        termsVersion: {
            versionMajor: {version_major},
            versionMinor: {version_minor}
        }
    })
    {
    acceptedVersion
  }
}"#;

#[allow(dead_code)]
pub const GET_TERMS_VERSION_QUERY: &str = r#"query {
    termsAndConditionsForProduct(productCode: "{product_code}") {
        name
        version
    }
}"#;

#[allow(dead_code)]
pub const CONSUMPTION_QUERY: &str = r#"query {
    smartMeterTelemetry(
        deviceId: "{device_id}"
        grouping: HALF_HOURLY
        start: "{start_date}"
        end: "{end_date}"
    ) {
    readAt
    consumptionDelta
    costDeltaWithTax
  }
}"#;

#[allow(dead_code)]
pub const ACCOUNT_QUERY: &str = r#"query{
    account(
        accountNumber: "{acc_number}"
    ) {
    electricityAgreements(active: true) {
        validFrom
        validTo
        meterPoint {
            meters(includeInactive: false) {
                smartDevices {
                    deviceId
                }
            }
            mpan
            direction
        }
        tariff {
            ... on HalfHourlyTariff {
                id
                productCode
                tariffCode
                productCode
                standingCharge
                }
            }
        }
    }
}"#;

#[allow(dead_code)]
pub const ENROLMENT_QUERY: &str = r#"query {
    productEnrolments(accountNumber: "{acc_number}") {
        id
        status
        product {
            code
            displayName
        }
    stages {
      name
      status
      steps {
        displayName
        status
        updatedAt
      }
    }
  }
}"#;

#[allow(dead_code)]
pub const SWITCH_QUERY: &str = r#"mutation {
  startOnboardingProcess(input: {
    accountNumber: "{account_number}",
    mpan: "{mpan}",
    productCode: "{product_code}",
    targetAgreementChangeDate: "{change_date}"
  })
  {
    onboardingProcess {
      id
    }
    productEnrolment {
      id
    }
  }
}"#;
