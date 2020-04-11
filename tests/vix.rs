use chrono::prelude::*;
use options_math::*;
use serde::Deserialize;
use std::collections::HashMap;
use std::error::Error;
use std::fs::File;

#[derive(Debug, Deserialize)]
struct Record {
    expiration: String,
    days: String,
    strike: String,
    call_bid: String,
    call_ask: String,
    put_bid: String,
    put_ask: String,
}

#[test]
fn test_vix() -> Result<(), Box<dyn Error>> {
    let f = File::open("./data/options.csv")?;
    let mut rdr = csv::Reader::from_reader(f);

    let now = NaiveDateTime::from_timestamp(1230768000, 0);

    let mut options: Vec<OptionContract> = vec![];

    for result in rdr.deserialize() {
        // Notice that we need to provide a type hint for automatic
        // deserialization.
        let record: Record = result?;

        let expiration = (now + chrono::Duration::days(record.days.parse::<i64>()?))
            .with_hour(16)
            .unwrap();

        options.push(OptionContract::new(
            expiration,
            (record.strike.parse::<f64>()? * 100.0) as Cents,
            OptionKind::Call,
            (record.call_bid.parse::<f64>()? * 100.0) as Cents,
            (record.call_ask.parse::<f64>()? * 100.0) as Cents,
        ));
        options.push(OptionContract::new(
            expiration,
            (record.strike.parse::<f64>()? * 100.0) as Cents,
            OptionKind::Put,
            (record.put_bid.parse::<f64>()? * 100.0) as Cents,
            (record.put_ask.parse::<f64>()? * 100.0) as Cents,
        ));
    }

    let options_by_expiry = group_options_by_expiry(&options[..]);

    let mut options_by_expiry_sorted: Vec<NaiveDateTime> =
        options_by_expiry.keys().map(|k| *k).collect();
    options_by_expiry_sorted.sort();

    match (
        options_by_expiry_sorted
            .get(0)
            .and_then(|d| options_by_expiry.get(d)),
        options_by_expiry_sorted
            .get(1)
            .and_then(|d| options_by_expiry.get(d)),
    ) {
        (Some(near_term), Some(next_term)) => {
            let vix = compute_vix(near_term, next_term, now);
            println!("{:?}", vix);
        }
        _ => {}
    }

    // println!("{:?}", options_by_expiry);
    // options_math::compute_vix();
    Ok(())
}
