use chrono::prelude::*;
use itertools::Itertools;
use std::collections::HashMap;

#[derive(PartialEq, Clone, Copy)]
pub enum OptionKind {
    Call,
    Put,
}

pub type Cents = i64;

pub type Percentage = f64;

#[derive(Clone, Copy)]
pub struct OptionContract {
    strike: Cents,
    kind: OptionKind,
    bid: Cents,
    ask: Cents,
}

impl OptionContract {
    /**
     * Mark price
     */
    pub fn mark(self) -> Cents {
        return (self.ask - self.bid) / 2;
    }
}

#[derive(Clone, Copy)]
struct OptionStrike {
    price: Cents,
    put: OptionContract,
    call: OptionContract,
    delta_k: Cents,
}

impl OptionStrike {
    /**
     * Difference between the price of the call and put
     */
    pub fn call_put_difference(self) -> Cents {
        return self.call.mark() - self.put.mark();
    }

    /**
     * The midpoint of the call mark price and put mark price.
     */
    pub fn mark(self) -> Cents {
        return (self.call.mark() + self.put.mark()) / 2;
    }
}

pub struct OptionsByExpiryDate {
    expires_at: NaiveDateTime,
    risk_free_rate: Percentage,
    calls: Vec<OptionContract>,
    puts: Vec<OptionContract>,
}

impl OptionsByExpiryDate {
    /**
     * Gets options grouped and sorted by their strike price.
     */
    fn get_strikes(&self) -> Vec<OptionStrike> {
        let all_options = self
            .calls
            .clone()
            .into_iter()
            .chain(self.puts.clone().into_iter());
        let mut options_by_strike: Vec<OptionStrike> = all_options
            .group_by(|o| o.strike)
            .into_iter()
            .flat_map(|(strike, mut options)| -> Option<OptionStrike> {
                let call = options.find(|o| o.kind == OptionKind::Call);
                let put = options.find(|o| o.kind == OptionKind::Put);
                return match (call, put) {
                    (Some(c), Some(p)) => Some(OptionStrike {
                        price: strike,
                        call: c,
                        put: p,
                        delta_k: 0,
                    }),
                    _ => None,
                };
            })
            .collect();
        options_by_strike.sort_unstable_by_key(|s| s.price);

        let mut delta_ks: HashMap<Cents, Cents> = HashMap::new();
        for w in options_by_strike.windows(3) {
            match (w.get(0), w.get(1), w.get(2)) {
                (Some(prev), Some(curr), Some(next)) => {
                    // Interval between strike prices – half the difference between the strike on either side of Ki:
                    let delta_k = (next.price - prev.price) / 2;
                    delta_ks.insert(curr.price, delta_k);
                }
                _ => {}
            };
        }

        return options_by_strike
            .into_iter()
            .map(|mut s| -> OptionStrike {
                s.delta_k = *delta_ks.get(&s.price).unwrap_or(&0);
                return s;
            })
            .collect();
    }

    /**
     * Computes the number of minutes until the option's expiration.
     */
    pub fn minutes_to_expiration(&self, now: NaiveDateTime) -> Percentage {
        return self.expires_at.signed_duration_since(now).num_minutes() as f64;
    }

    /**
     * Computes the time to the option's expiration as a percentage of the remaining year.
     */
    pub fn time_to_expiration(&self, now: NaiveDateTime) -> Percentage {
        return self.minutes_to_expiration(now) / 525600.0;
    }

    /**
     * Computes the implied forward price.
     */
    pub fn forward_price(&self, now: NaiveDateTime) -> Cents {
        let interest = (self.risk_free_rate * self.time_to_expiration(now)).exp();
        let mut strikes = self.get_strikes();
        // we want to find the ATM option
        strikes.sort_unstable_by_key(|k| k.call_put_difference().abs());
        let atm = strikes.first();
        return atm
            .map(|strike| -> Cents {
                strike.price + (interest * strike.call_put_difference() as f64) as Cents
            })
            .unwrap_or(0);
    }

    /**
     * \sigma^2 from the VIX whitepaper
     */
    pub fn variance(&self, now: NaiveDateTime) -> Percentage {
        let risk_free_interest = (self.risk_free_rate * self.time_to_expiration(now)).exp();
        let strikes = self.get_strikes();
        let fp = self.forward_price(now);
        let (mut below_and_k, above): (Vec<OptionStrike>, Vec<OptionStrike>) =
            strikes.into_iter().partition(|x| (*x).price < fp);

        // The highest below the forward price is K
        below_and_k.sort_unstable_by_key(|k| -k.price);
        let (below, k) = below_and_k.split_at(1);
        let k_0 = k.first().map(|s| s.price).unwrap_or(0);

        // find all out of the money options + the atm option
        let selected_options = below
            .into_iter()
            .map(|s| (s.put, s.delta_k))
            .chain(above.into_iter().map(|s| (s.call, s.delta_k)))
            .chain(
                k.into_iter()
                    .flat_map(|s| vec![(s.call, s.delta_k), (s.put, s.delta_k)]),
            )
            .collect::<Vec<(OptionContract, Cents)>>();

        let contributions: f64 = selected_options
            .into_iter()
            .map(|(option, delta_k)| -> f64 {
                return (delta_k as f64) / ((option.strike * option.strike) as f64)
                    * (option.mark() as f64)
                    * (risk_free_interest as f64);
            })
            .sum();

        let a = fp as f64 / k_0 as f64 - 1.0;
        return (2.0 * contributions - a * a) / self.time_to_expiration(now);
    }
}

pub fn compute_vix(
    near_term: &OptionsByExpiryDate,
    next_term: &OptionsByExpiryDate,
    now: NaiveDateTime,
) -> Percentage {
    let t1 = near_term.time_to_expiration(now);
    let n_t1 = near_term.minutes_to_expiration(now);
    let s1_sq = near_term.variance(now);
    let t2 = next_term.time_to_expiration(now);
    let n_t2 = next_term.minutes_to_expiration(now);
    let s2_sq = next_term.variance(now);
    let n_30 = (30 * 24 * 60) as f64;
    let n_365 = (365 * 24 * 60) as f64;
    return (t1 * s1_sq * (n_t2 - n_30) / (n_t2 - n_t1)
        + t2 * s2_sq * (n_30 - n_t1) / (n_t2 - n_t1))
        .powf(0.5)
        * 100.0;
}

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}
