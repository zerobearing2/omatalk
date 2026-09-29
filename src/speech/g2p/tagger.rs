//! Port of spaCy en_core_web_sm 3.8's tok2vec + tagger, the pipeline misaki
//! loads (thinc MultiHashEmbed -> MaxoutWindowEncoder -> Softmax). Weights,
//! spaCy's symbol table, lexeme_norm and the tokenizer's NORM exceptions come
//! from the embedded `data/tagger.bin` (tools/parity/export_tagger.py, MIT
//! weights).
// The parity-tested, autovectorized loops use `chunks_exact`.
#![allow(clippy::chunks_exact_to_as_chunks)]
use std::collections::HashMap;

use crate::speech::LoadError;

const WIDTH: usize = 96;
const PIECES: usize = 3;
const DEPTH: usize = 4;

struct Embed {
    rows: usize,
    seed: u32,
    table: Vec<f32>,
}

struct Maxout {
    inputs: usize,
    wt: Vec<f32>,
    b: Vec<f32>,
    g: Vec<f32>,
    beta: Vec<f32>,
}

pub struct Tagger {
    labels: Vec<String>,
    embeds: Vec<Embed>,
    layers: Vec<Maxout>,
    out_w: Vec<f32>,
    out_b: Vec<f32>,
    symbols: HashMap<String, u64>,
    norms: HashMap<u64, String>,
    classes: [Vec<(u32, u32)>; 3],
    exceptions: HashMap<String, Vec<Vec<(String, String)>>>,
}

struct Reader<'a> {
    bytes: &'a [u8],
}

fn bad(why: &str) -> LoadError {
    LoadError(format!("tagger.bin: {why}"))
}

impl Reader<'_> {
    fn take(&mut self, n: usize) -> Result<&[u8], LoadError> {
        let (head, rest) = self
            .bytes
            .split_at_checked(n)
            .ok_or_else(|| bad("truncated"))?;
        self.bytes = rest;
        Ok(head)
    }

    fn u32(&mut self) -> Result<u32, LoadError> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }

    fn u64(&mut self) -> Result<u64, LoadError> {
        Ok(u64::from_le_bytes(self.take(8)?.try_into().unwrap()))
    }

    fn string(&mut self) -> Result<String, LoadError> {
        let n = self.u32()? as usize;
        String::from_utf8(self.take(n)?.to_vec()).map_err(|_| bad("string is not UTF-8"))
    }

    fn f32s(&mut self, n: usize) -> Result<Vec<f32>, LoadError> {
        Ok(self
            .take(n * 4)?
            .chunks_exact(4)
            .map(|c| f32::from_le_bytes(c.try_into().unwrap()))
            .collect())
    }

    fn weights(&mut self, n: usize, half: bool) -> Result<Vec<f32>, LoadError> {
        if !half {
            return self.f32s(n);
        }
        Ok(self
            .take(n * 2)?
            .chunks_exact(2)
            .map(|c| f16(u16::from_le_bytes([c[0], c[1]])))
            .collect())
    }

    /// `n` items, where `n` is the next u32 in the stream.
    fn list<T>(
        &mut self,
        mut item: impl FnMut(&mut Self) -> Result<T, LoadError>,
    ) -> Result<Vec<T>, LoadError> {
        let n = self.u32()?;
        (0..n).map(|_| item(self)).collect()
    }
}

fn f16(h: u16) -> f32 {
    let sign = ((h >> 15) as u32) << 31;
    let exp = ((h >> 10) & 0x1f) as u32;
    let frac = (h & 0x3ff) as u32;
    let bits = match exp {
        0 if frac == 0 => sign,
        0 => {
            let v = frac as f32 * 2f32.powi(-24);
            return if sign != 0 { -v } else { v };
        }
        0x1f => sign | 0x7f80_0000 | (frac << 13),
        _ => sign | ((exp + 112) << 23) | (frac << 13),
    };
    f32::from_bits(bits)
}

// MurmurHash64A, seed 1: spaCy's string hash.
fn hash_string(s: &str) -> u64 {
    const M: u64 = 0xc6a4a7935bd1e995;
    let data = s.as_bytes();
    let mut h = 1u64 ^ (data.len() as u64).wrapping_mul(M);
    let chunks = data.chunks_exact(8);
    let tail = chunks.remainder();
    for c in chunks {
        let mut k = u64::from_le_bytes(c.try_into().unwrap()).wrapping_mul(M);
        k ^= k >> 47;
        h = (h ^ k.wrapping_mul(M)).wrapping_mul(M);
    }
    if !tail.is_empty() {
        for (i, &b) in tail.iter().enumerate() {
            h ^= (b as u64) << (8 * i);
        }
        h = h.wrapping_mul(M);
    }
    h ^= h >> 47;
    h = h.wrapping_mul(M);
    h ^ (h >> 47)
}

// thinc's MurmurHash3_x86_128_uint64 (really the x64 finalizer over one key).
fn hash_key(val: u64, seed: u32) -> [u32; 4] {
    fn fmix(mut k: u64) -> u64 {
        k ^= k >> 33;
        k = k.wrapping_mul(0xff51afd7ed558ccd);
        k ^= k >> 33;
        k = k.wrapping_mul(0xc4ceb9fe1a85ec53);
        k ^ (k >> 33)
    }
    let mut h1 = val
        .wrapping_mul(0x87c37b91114253d5)
        .rotate_left(31)
        .wrapping_mul(0x4cf5ad432745937f);
    h1 ^= seed as u64 ^ 8;
    let mut h2 = seed as u64 ^ 8;
    h1 = h1.wrapping_add(h2);
    h2 = h2.wrapping_add(h1);
    h1 = fmix(h1);
    h2 = fmix(h2);
    h1 = h1.wrapping_add(h2);
    h2 = h2.wrapping_add(h1);
    [h1 as u32, (h1 >> 32) as u32, h2 as u32, (h2 >> 32) as u32]
}

fn within(ranges: &[(u32, u32)], c: char) -> bool {
    let c = c as u32;
    let i = ranges.partition_point(|&(lo, _)| lo <= c);
    i > 0 && ranges[i - 1].1 >= c
}

fn shape(text: &str, classes: &[Vec<(u32, u32)>; 3]) -> String {
    if text.chars().count() >= 100 {
        return "LONG".into();
    }
    let mut out = String::new();
    let (mut last, mut seq) = ('\0', 0);
    for c in text.chars() {
        let [alpha, upper, digit] = classes;
        let s = if within(alpha, c) {
            if within(upper, c) { 'X' } else { 'x' }
        } else if within(digit, c) {
            'd'
        } else {
            c
        };
        if s == last {
            seq += 1;
        } else {
            seq = 0;
            last = s;
        }
        if seq < 4 {
            out.push(s);
        }
    }
    out
}

fn dot(a: &[f32], b: &[f32]) -> f32 {
    let mut acc = [0f32; 8];
    for (x, y) in a.chunks_exact(8).zip(b.chunks_exact(8)) {
        for i in 0..8 {
            acc[i] += x[i] * y[i];
        }
    }
    acc.iter().sum()
}

const BLOCK: usize = 8;

// out[r] = x[r * stride..][..n] * wt + b, with wt the transposed weights
// (n x outs) so the inner loop is a vectorizable axpy. Each weight row is
// applied to BLOCK rows while it sits in L1; out is rounded up to BLOCK rows
// and the spare rows repeat the last input.
#[inline(always)]
fn affine(wt: &[f32], b: &[f32], x: &[f32], stride: usize, rows: usize, out: &mut [f32]) {
    let outs = b.len();
    for (r0, block) in out
        .chunks_exact_mut(BLOCK * outs)
        .enumerate()
        .map(|(i, o)| (i * BLOCK, o))
    {
        for o in block.chunks_exact_mut(outs) {
            o.copy_from_slice(b);
        }
        let at: [usize; BLOCK] = std::array::from_fn(|j| (r0 + j).min(rows - 1) * stride);
        for (i, w) in wt.chunks_exact(outs).enumerate() {
            for (o, &a) in block.chunks_exact_mut(outs).zip(&at) {
                let xi = x[a + i];
                for (o, &wk) in o.iter_mut().zip(w) {
                    *o += xi * wk;
                }
            }
        }
    }
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
unsafe fn affine_avx2(
    wt: &[f32],
    b: &[f32],
    x: &[f32],
    stride: usize,
    rows: usize,
    out: &mut [f32],
) {
    affine(wt, b, x, stride, rows, out)
}

impl Maxout {
    fn forward(
        &self,
        x: &[f32],
        stride: usize,
        rows: usize,
        pieces: &mut Vec<f32>,
        out: &mut [f32],
    ) {
        pieces.resize(rows.next_multiple_of(BLOCK) * WIDTH * PIECES, 0.0);
        #[cfg(target_arch = "x86_64")]
        if is_x86_feature_detected!("avx2") {
            unsafe { affine_avx2(&self.wt, &self.b, x, stride, rows, pieces) };
        } else {
            affine(&self.wt, &self.b, x, stride, rows, pieces);
        }
        #[cfg(not(target_arch = "x86_64"))]
        affine(&self.wt, &self.b, x, stride, rows, pieces);
        for (y, p) in out
            .chunks_exact_mut(WIDTH)
            .zip(pieces.chunks_exact(WIDTH * PIECES))
        {
            for (o, v) in y.iter_mut().enumerate() {
                *v = p[o * PIECES..(o + 1) * PIECES]
                    .iter()
                    .copied()
                    .fold(f32::NEG_INFINITY, f32::max);
            }
            let mean = y.iter().sum::<f32>() / WIDTH as f32;
            let var = y.iter().map(|v| (v - mean) * (v - mean)).sum::<f32>() / WIDTH as f32 + 1e-8;
            let scale = var.sqrt().recip();
            for (i, v) in y.iter_mut().enumerate() {
                *v = (*v - mean) * scale * self.g[i] + self.beta[i];
            }
        }
    }
}

impl Tagger {
    pub fn load() -> Result<Tagger, LoadError> {
        Tagger::from_bytes(include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/data/tagger.bin"
        )))
    }

    fn from_bytes(bytes: &[u8]) -> Result<Tagger, LoadError> {
        let body = bytes
            .strip_prefix(b"TAG1")
            .ok_or_else(|| bad("bad magic"))?;
        let mut r = Reader { bytes: body };
        let halves = r.u32()?;
        let labels = r.list(Reader::string)?;
        let embeds = r.list(|r| {
            let rows = r.u32()? as usize;
            let seed = r.u32()?;
            Ok(Embed {
                rows,
                seed,
                table: r.weights(rows * WIDTH, halves & 1 != 0)?,
            })
        })?;
        let layers = r.list(|r| {
            let inputs = r.u32()? as usize;
            let w = r.weights(WIDTH * PIECES * inputs, halves & 2 != 0)?;
            let outs = WIDTH * PIECES;
            Ok(Maxout {
                inputs,
                wt: (0..inputs * outs)
                    .map(|j| w[(j % outs) * inputs + j / outs])
                    .collect(),
                b: r.f32s(WIDTH * PIECES)?,
                g: r.f32s(WIDTH)?,
                beta: r.f32s(WIDTH)?,
            })
        })?;
        let out_w = r.f32s(labels.len() * WIDTH)?;
        let out_b = r.f32s(labels.len())?;
        let symbols = r
            .list(|r| Ok((r.string()?, r.u32()? as u64)))?
            .into_iter()
            .collect();
        let norms = r
            .list(|r| Ok((r.u64()?, r.string()?)))?
            .into_iter()
            .collect();
        let classes = [
            r.list(|r| Ok((r.u32()?, r.u32()?)))?,
            r.list(|r| Ok((r.u32()?, r.u32()?)))?,
            r.list(|r| Ok((r.u32()?, r.u32()?)))?,
        ];
        let mut exceptions: HashMap<String, Vec<Vec<(String, String)>>> = HashMap::new();
        for pieces in r.list(|r| r.list(|r| Ok((r.string()?, r.string()?))))? {
            let first = pieces
                .first()
                .ok_or_else(|| bad("empty exception"))?
                .0
                .clone();
            exceptions.entry(first).or_default().push(pieces);
        }
        if !r.bytes.is_empty() || layers.len() != DEPTH + 1 {
            return Err(bad("unexpected layout"));
        }
        Ok(Tagger {
            labels,
            embeds,
            layers,
            out_w,
            out_b,
            symbols,
            norms,
            classes,
            exceptions,
        })
    }

    fn string_id(&self, s: &str) -> u64 {
        match self.symbols.get(s) {
            Some(&id) => id,
            None if s.is_empty() => 0,
            None => hash_string(s),
        }
    }

    // Token.norm: a tokenizer special case's NORM, else lexeme_norm, else lower().
    fn norms(&self, words: &[&str], spaces: &[bool]) -> Vec<String> {
        let mut norms: Vec<String> = words
            .iter()
            .map(|w| match self.norms.get(&self.string_id(w)) {
                Some(n) => n.clone(),
                None => w.to_lowercase(),
            })
            .collect();
        let mut i = 0;
        while i < words.len() {
            let fits = |rule: &&Vec<(String, String)>| {
                i + rule.len() <= words.len()
                    && rule
                        .iter()
                        .enumerate()
                        .all(|(j, (orth, _))| words[i + j] == orth)
                    && !spaces[i..i + rule.len() - 1].contains(&true)
            };
            let rule = self
                .exceptions
                .get(words[i])
                .and_then(|rules| rules.iter().filter(fits).max_by_key(|r| r.len()));
            let Some(rule) = rule else {
                i += 1;
                continue;
            };
            for (j, (_, norm)) in rule.iter().enumerate() {
                if !norm.is_empty() {
                    norms[i + j] = norm.clone();
                }
            }
            i += rule.len();
        }
        norms
    }

    // spaces[i] says token i is followed by whitespace: spaCy embeds it.
    pub fn tag(&self, words: &[&str], spaces: &[bool]) -> Vec<String> {
        let n = words.len();
        if n == 0 {
            return Vec::new();
        }
        let norms = self.norms(words, spaces);
        let mut concat = vec![0f32; n * self.embeds.len() * WIDTH];
        for (t, (word, slots)) in words
            .iter()
            .zip(concat.chunks_exact_mut(self.embeds.len() * WIDTH))
            .enumerate()
        {
            let chars: Vec<char> = word.chars().collect();
            let prefix: String = chars.iter().take(1).collect();
            let suffix: String = chars[chars.len().saturating_sub(3)..].iter().collect();
            let keys = [
                self.string_id(&norms[t]),
                self.string_id(&prefix),
                self.string_id(&suffix),
                self.string_id(&shape(word, &self.classes)),
                spaces[t] as u64,
                (!word.is_empty() && word.chars().all(char::is_whitespace)) as u64,
            ];
            for ((embed, key), slot) in self
                .embeds
                .iter()
                .zip(keys)
                .zip(slots.chunks_exact_mut(WIDTH))
            {
                for h in hash_key(key, embed.seed) {
                    let row = (h as usize % embed.rows) * WIDTH;
                    for (s, v) in slot.iter_mut().zip(&embed.table[row..row + WIDTH]) {
                        *s += v;
                    }
                }
            }
        }
        // thinc's with_array(pad=4) wraps the doc in DEPTH zero rows that the
        // encoder layers also transform, so edge tokens see non-zero context.
        // Layer d only needs the rows within DEPTH - d of the doc.
        let rows = n + 2 * DEPTH;
        let mut x = vec![0f32; rows * WIDTH];
        let mut pieces = Vec::new();
        let first = &self.layers[0];
        first.forward(
            &concat,
            first.inputs,
            n,
            &mut pieces,
            &mut x[DEPTH * WIDTH..(DEPTH + n) * WIDTH],
        );
        let mut window = vec![0f32; (rows + 2) * WIDTH];
        let mut y = vec![0f32; rows * WIDTH];
        for (d, layer) in self.layers[1..].iter().enumerate() {
            window[WIDTH..(rows + 1) * WIDTH].copy_from_slice(&x);
            let (lo, hi) = (d + 1, rows - d - 1);
            layer.forward(
                &window[lo * WIDTH..],
                WIDTH,
                hi - lo,
                &mut pieces,
                &mut y[..(hi - lo) * WIDTH],
            );
            for (a, b) in x[lo * WIDTH..hi * WIDTH].iter_mut().zip(&y) {
                *a += b;
            }
        }
        (0..n)
            .map(|t| {
                let h = &x[(t + DEPTH) * WIDTH..(t + DEPTH + 1) * WIDTH];
                let best = (0..self.labels.len())
                    .map(|l| {
                        (
                            l,
                            dot(&self.out_w[l * WIDTH..(l + 1) * WIDTH], h) + self.out_b[l],
                        )
                    })
                    .fold((0, f32::NEG_INFINITY), |a, b| if b.1 > a.1 { b } else { a })
                    .0;
                self.labels[best].clone()
            })
            .collect()
    }
}
