// obtains the country codes for CountryCodes.json
// from Wikidata
use std::fs::File;

use minidom::Element;
use reqwest;
use serde_json;
use url::Url;


const SPARQL_QUERY: &'static str = r#"
# Wikidata
SELECT ?alpha3code ?alpha2code ?platecode ?countryLabel
WHERE
{
  # country is-a sovereign state
  ?country wdt:P31 wd:Q3624078.
  # country has-an alpha-3 code
  ?country wdt:P298 ?alpha3code.
  OPTIONAL {
    # country has-an alpha-2 code
    ?country wdt:P297 ?alpha2code.
  }
  OPTIONAL {
    # country has-a license plate code
    ?country wdt:P395 ?platecode.
  }
  SERVICE wikibase:label { bd:serviceParam wikibase:language "[AUTO_LANGUAGE],en". }
}
"#;
const SPARQL_ENDPOINT: &'static str = "https://query.wikidata.org/sparql";
const SPARQL_QUERY_KEY: &'static str = "query";
const SPARQL_NS: &'static str = "http://www.w3.org/2005/sparql-results#";

// resulting format:
//<?xml version='1.0' encoding='UTF-8'?>
//<sparql xmlns='http://www.w3.org/2005/sparql-results#'>
//  <head>
//    <variable name='countryLabel' />
//    <variable name='alpha3code' />
//    <variable name='alpha2code' />
//    <variable name='platecode' />
//  </head>
//  <results>
//    <result>
//      <binding name='alpha2code'>
//        <literal>CG</literal>
//      </binding>
//      <binding name='platecode'>
//        <literal>RCB</literal>
//      </binding>
//      <binding name='alpha3code'>
//        <literal>COG</literal>
//      </binding>
//      <binding name='countryLabel'>
//        <literal xml:lang='en'>Republic of the Congo</literal>
//      </binding>
//    </result>
//    <result>
//      <binding name='alpha2code'>
//        <literal>CD</literal>
//      </binding>
//      <binding name='platecode'>
//        <literal>CGO</literal>
//      </binding>
//      <binding name='alpha3code'>
//        <literal>COD</literal>
//      </binding>
//      <binding name='countryLabel'>
//        <literal xml:lang='en'>Democratic Republic of the Congo</literal>
//      </binding>
//    </result>
//    <!-- ... -->
//  </results>
//</sparql>


fn minimize_sparql(query: &str) -> String {
    let mut minimized = String::new();

    let mut eating_whitespace = false;
    let mut eating_comment = false;
    for c in query.chars() {
        if eating_comment {
            if c != '\n' {
                continue;
            }

            // newline after comment => end of comment
            eating_comment = false;
        } else {
            if c.is_whitespace() {
                // fold sequences of whitespace into a single space character
                if !eating_whitespace {
                    minimized.push(' ');
                    eating_whitespace = true;
                }
            } else if c == '#' {
                eating_comment = true;
            } else {
                eating_whitespace = false;
                minimized.push(c);
            }
        }
    }

    minimized
}


#[tokio::main]
async fn main() {
    let minimized_query = minimize_sparql(SPARQL_QUERY);

    let mut sparql_uri: Url = SPARQL_ENDPOINT.parse()
        .expect("failed to parse SPARQL URI");
    sparql_uri.query_pairs_mut()
        .append_pair(SPARQL_QUERY_KEY, minimized_query.trim());

    let response = reqwest::Client::new()
        .get(sparql_uri)
        .header("User-Agent", "rocketbot_geonames/countrycodes https://github.com/RavuAlHemio/rocketbot")
        .send().await.expect("failed to obtain SPARQL result")
        .bytes().await.expect("failed to obtain bytes")
        .to_vec();
    let response_string = String::from_utf8(response)
        .expect("SPARQL result is not valid UTF-8");

    let root: Element = response_string.parse().expect("failed to parse XML");
    let results_elem = root.get_child("results", SPARQL_NS)
        .expect("no results element");

    let mut countries = Vec::new();
    for result_elem in results_elem.children() {
        if !result_elem.is("result", SPARQL_NS) {
            continue;
        }

        let mut country = serde_json::json!({});
        country["country"] = serde_json::Value::Null;
        country["plate"] = serde_json::Value::Null;
        country["alpha2"] = serde_json::Value::Null;
        country["alpha3"] = serde_json::Value::Null;

        for binding_elem in result_elem.children() {
            if !binding_elem.is("binding", SPARQL_NS) {
                continue;
            }
            let name = binding_elem
                .attr("name").expect("\"binding\" element does not have a \"name\" attribute");
            let literal_value = binding_elem
                .get_child("literal", SPARQL_NS).expect("\"binding\" element does not have a \"literal\" child")
                .text();

            match name {
                "countryLabel" => { country["country"] = serde_json::Value::String(literal_value) },
                "platecode" => { country["plate"] = serde_json::Value::String(literal_value) },
                "alpha2code" => { country["alpha2"] = serde_json::Value::String(literal_value) },
                "alpha3code" => { country["alpha3"] = serde_json::Value::String(literal_value) },
                _ => {},
            };
        }

        countries.push(country);
    }

    countries.sort_unstable_by_key(|c| (
        c["alpha3"].as_str().map(|s| s.to_owned()),
        c["alpha2"].as_str().map(|s| s.to_owned()),
    ));

    let countries_json = serde_json::Value::Array(countries);

    {
        let mut file = File::create("CountryCodes.json")
            .expect("failed to create file");
        serde_json::to_writer_pretty(&mut file, &countries_json)
            .expect("failed to write file");
    }
}
