//! Regression gate. A benchmark fails only when the PR is slower than main
//! on both tests: the 95% confidence intervals of the two medians do not
//! overlap, and a one-sided Mann-Whitney test stays significant after Holm
//! correction across the batch. There is no percentage tolerance.

#[derive(Clone, Debug)]
pub struct Verdict {
    pub name: String,
    pub slower: bool,
    pub intervals_disjoint: bool,
    pub p_value: f64,
    pub p_holm: f64,
}

pub fn regression(batches: &[(String, Vec<f64>, Vec<f64>)]) -> Vec<Verdict> {
    let mut rows = Vec::new();
    for (name, main, pr) in batches {
        let (_main_lo, main_hi) = median_ci(main);
        let (pr_lo, _pr_hi) = median_ci(pr);
        let disjoint = pr_lo > main_hi && main_hi < pr_lo;
        let p = mann_whitney_greater(pr, main);
        rows.push(Verdict { name: name.clone(), slower: false, intervals_disjoint: disjoint, p_value: p, p_holm: p });
    }
    let adjusted = holm(&rows.iter().map(|r| r.p_value).collect::<Vec<_>>());
    for (row, p) in rows.iter_mut().zip(adjusted) {
        row.p_holm = p;
        row.slower = row.intervals_disjoint && p < 0.05;
    }
    rows
}

pub fn failed(rows: &[Verdict]) -> bool { rows.iter().any(|r| r.slower) }

/// Order-statistic interval for the median. For n >= 6 this is the central
/// span whose binomial coverage is at least 95%.
pub fn median_ci(sample: &[f64]) -> (f64, f64) {
    if sample.is_empty() { return (0.0, 0.0); }
    let mut xs = sample.to_vec();
    xs.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let n = xs.len();
    if n < 6 { return (xs[0], xs[n - 1]); }
    let mut best = (0usize, n - 1);
    for k in 1..=n / 2 {
        let lo = k - 1;
        let hi = n - k;
        if coverage(n, lo, hi) >= 0.95 { best = (lo, hi); }
    }
    (xs[best.0], xs[best.1])
}

fn coverage(n: usize, lo: usize, hi: usize) -> f64 {
    // P(X_(lo+1) <= median <= X_(hi+1)) ≈ sum_{k=lo+1}^{hi} Bin(n, 0.5)
    let mut total = 0.0;
    for k in (lo + 1)..=hi.min(n) {
        total += binom_pmf(n, k);
    }
    total
}

fn binom_pmf(n: usize, k: usize) -> f64 {
    if k > n { return 0.0; }
    let mut c = 1.0f64;
    for i in 0..k { c *= (n - i) as f64 / (i + 1) as f64; }
    c / 2f64.powi(n as i32)
}

pub fn mann_whitney_greater(x: &[f64], y: &[f64]) -> f64 {
    let n1 = x.len() as f64;
    let n2 = y.len() as f64;
    if n1 == 0.0 || n2 == 0.0 { return 1.0; }
    let mut all: Vec<(f64, u8)> = x.iter().map(|v| (*v, 0)).chain(y.iter().map(|v| (*v, 1))).collect();
    all.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
    let mut ranks = vec![0.0; all.len()];
    let mut i = 0;
    while i < all.len() {
        let mut j = i;
        while j + 1 < all.len() && all[j + 1].0 == all[i].0 { j += 1; }
        let rank = ((i + 1) + (j + 1)) as f64 / 2.0;
        for slot in ranks.iter_mut().take(j + 1).skip(i) { *slot = rank; }
        i = j + 1;
    }
    let r1: f64 = ranks.iter().enumerate().filter(|(i, _)| all[*i].1 == 0).map(|(_, r)| *r).sum();
    let u = r1 - n1 * (n1 + 1.0) / 2.0;
    let mean = n1 * n2 / 2.0;
    let mut tie = 0.0;
    i = 0;
    while i < all.len() {
        let mut j = i;
        while j + 1 < all.len() && all[j + 1].0 == all[i].0 { j += 1; }
        let t = (j - i + 1) as f64;
        if t > 1.0 { tie += t * t * t - t; }
        i = j + 1;
    }
    let n = n1 + n2;
    let var = n1 * n2 / 12.0 * ((n + 1.0) - tie / (n * (n - 1.0)));
    if var <= 0.0 { return if u > mean { 0.0 } else { 1.0 }; }
    let z = (u - mean - 0.5) / var.sqrt();
    1.0 - phi(z)
}

fn phi(z: f64) -> f64 {
    0.5 * (1.0 + erf(z / std::f64::consts::SQRT_2))
}

fn erf(x: f64) -> f64 {
    let sign = if x < 0.0 { -1.0 } else { 1.0 };
    let x = x.abs();
    let a1 = 0.254829592;
    let a2 = -0.284496736;
    let a3 = 1.421413741;
    let a4 = -1.453152027;
    let a5 = 1.061405429;
    let p = 0.3275911;
    let t = 1.0 / (1.0 + p * x);
    let y = 1.0 - (((((a5 * t + a4) * t) + a3) * t + a2) * t + a1) * t * (-x * x).exp();
    sign * y
}

pub fn holm(p_values: &[f64]) -> Vec<f64> {
    let m = p_values.len();
    let mut order: Vec<usize> = (0..m).collect();
    order.sort_by(|&a, &b| p_values[a].partial_cmp(&p_values[b]).unwrap_or(std::cmp::Ordering::Equal));
    let mut adjusted = vec![1.0; m];
    let mut running: f64 = 0.0;
    for (rank, index) in order.iter().enumerate() {
        let adj = ((m - rank) as f64 * p_values[*index]).min(1.0);
        running = running.max(adj);
        adjusted[*index] = running;
    }
    adjusted
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aa_run_does_not_fail() {
        let sample = vec![10.0, 11.0, 10.5, 10.2, 10.8, 11.1, 10.4, 10.6, 10.9, 10.3, 10.7, 11.2, 10.1, 10.55, 10.45];
        let rows = regression(&[("startup".into(), sample.clone(), sample)]);
        assert!(!failed(&rows), "{:?}", rows[0].p_value);
    }

    #[test]
    fn injected_slowdown_fails() {
        let main = vec![10.0, 10.2, 10.1, 10.3, 9.9, 10.4, 10.0, 10.2, 10.1, 10.3, 9.8, 10.5, 10.0, 10.2, 10.1];
        let pr: Vec<f64> = main.iter().map(|v| v + 8.0).collect();
        let rows = regression(&[("startup".into(), main, pr)]);
        assert!(rows[0].intervals_disjoint, "{:?}", rows[0]);
        assert!(rows[0].p_holm < 0.05, "{}", rows[0].p_holm);
        assert!(failed(&rows));
    }

    #[test]
    fn overlapping_noise_does_not_fail() {
        let main = vec![10.0, 11.0, 10.5, 12.0, 9.5, 10.8, 11.2, 10.1, 10.9, 11.5, 9.8, 10.4, 11.1, 10.6, 10.3];
        let pr = vec![10.1, 11.1, 10.4, 12.1, 9.6, 10.7, 11.3, 10.0, 11.0, 11.4, 9.9, 10.5, 11.0, 10.7, 10.2];
        let rows = regression(&[("startup".into(), main, pr)]);
        assert!(!failed(&rows));
    }
}
