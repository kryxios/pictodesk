//! PSD import completeness: ZIP and ZIP+prediction channels,
//! 16-bit files, Indexed color, layer masks, and PSB.

fn hex(s: &str) -> Vec<u8> {
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap())
        .collect()
}

struct B {
    buf: Vec<u8>,
    psb: bool,
}
impl B {
    fn new(
        w: u32,
        h: u32,
        depth: u16,
        mode: u16,
        channels: u16,
        color_data: &[u8],
        psb: bool,
    ) -> B {
        let mut b = B {
            buf: Vec::new(),
            psb,
        };
        b.buf.extend_from_slice(b"8BPS");
        b.buf
            .extend_from_slice(&(if psb { 2u16 } else { 1u16 }).to_be_bytes());
        b.buf.extend_from_slice(&[0; 6]);
        b.buf.extend_from_slice(&channels.to_be_bytes());
        b.buf.extend_from_slice(&h.to_be_bytes());
        b.buf.extend_from_slice(&w.to_be_bytes());
        b.buf.extend_from_slice(&depth.to_be_bytes());
        b.buf.extend_from_slice(&mode.to_be_bytes());
        b.buf
            .extend_from_slice(&(color_data.len() as u32).to_be_bytes());
        b.buf.extend_from_slice(color_data);
        b.buf.extend_from_slice(&0u32.to_be_bytes()); // resources
        b
    }
    fn len64(&mut self, v: usize) {
        if self.psb {
            self.buf.extend_from_slice(&(v as u64).to_be_bytes());
        } else {
            self.buf.extend_from_slice(&(v as u32).to_be_bytes());
        }
    }
    /// Wrap layer records + channel data in the section lengths.
    fn finish(mut self, records_and_data: Vec<u8>) -> Vec<u8> {
        let info_len = records_and_data.len() + 4; // + global mask word
        let mut out = Vec::new();
        // section length placeholder then layer info length
        let sec_len = 8 + info_len; // two length words (PSD) approx; patched below
        let _ = sec_len;
        // Build: [section len][layer info len][records+data][global mask u32]
        let mut body = Vec::new();
        body.extend_from_slice(&records_and_data);
        body.extend_from_slice(&0u32.to_be_bytes());
        let mut head = Vec::new();
        if self.psb {
            head.extend_from_slice(&((8 + body.len()) as u64).to_be_bytes());
            head.extend_from_slice(&(body.len() as u64).to_be_bytes());
        } else {
            head.extend_from_slice(&((4 + body.len()) as u32).to_be_bytes());
            head.extend_from_slice(&(body.len() as u32).to_be_bytes());
        }
        out.append(&mut self.buf);
        out.append(&mut head);
        out.append(&mut body);
        out
    }
}

fn record(
    b: &mut B,
    name: &str,
    rect: (u32, u32, u32, u32),
    channels: &[(i16, usize)],
    key: &[u8; 4],
    opacity: u8,
) {
    b.buf.extend_from_slice(&rect.0.to_be_bytes()); // top
    b.buf.extend_from_slice(&rect.1.to_be_bytes()); // left
    b.buf.extend_from_slice(&rect.2.to_be_bytes()); // bottom
    b.buf.extend_from_slice(&rect.3.to_be_bytes()); // right
    b.buf
        .extend_from_slice(&(channels.len() as u16).to_be_bytes());
    for (id, len) in channels {
        b.buf.extend_from_slice(&id.to_be_bytes());
        b.len64(*len);
    }
    b.buf.extend_from_slice(b"8BIM");
    b.buf.extend_from_slice(key);
    b.buf.push(opacity);
    b.buf.push(0);
    b.buf.push(0);
    b.buf.push(0);
    let name_field = 1 + name.len();
    let pad = (4 - name_field % 4) % 4;
    let extra = 4 + 4 + name_field + pad;
    b.buf.extend_from_slice(&(extra as u32).to_be_bytes());
    b.buf.extend_from_slice(&0u32.to_be_bytes());
    b.buf.extend_from_slice(&0u32.to_be_bytes());
    b.buf.push(name.len() as u8);
    b.buf.extend_from_slice(name.as_bytes());
    b.buf.extend(std::iter::repeat_n(0u8, pad));
}

fn zip_chan(stream: &[u8]) -> Vec<u8> {
    let mut v = Vec::new();
    v.extend_from_slice(&2u16.to_be_bytes());
    v.extend_from_slice(stream);
    v
}

#[test]
fn psd_zip_channels() {
    // One 4x2 layer, all channels ZIP-compressed.
    let alpha = zip_chan(&hex("78dafbff1f020023e407f9"));
    let r = zip_chan(&hex("78da3b710202001c280641"));
    let g = zip_chan(&hex("78da9393830000044000f1"));
    let bl = zip_chan(&hex("78da4b4e8600000df40319"));
    let mut rec = B {
        buf: Vec::new(),
        psb: false,
    };
    rec.buf.extend_from_slice(&1i32.to_be_bytes());
    record(
        &mut rec,
        "Zip",
        (0, 0, 2, 4),
        &[(-1, alpha.len()), (0, r.len()), (1, g.len()), (2, bl.len())],
        b"norm",
        255,
    );
    let mut data = rec.buf;
    for c in [&alpha, &r, &g, &bl] {
        data.extend_from_slice(c);
    }
    let bytes = B::new(4, 2, 8, 3, 3, &[], false).finish(data);
    let (doc, report) = pictodesk_core::psd::load_psd(&bytes).expect("zip psd");
    assert_eq!(report.layers_imported, 1);
    let px = doc.layers[0].get_pixel(3, 1).to_rgba8();
    assert_eq!(px, (200, 30, 99, 255));
}

#[test]
fn psd_zip_prediction() {
    // 4x1 layer; red channel ZIP+prediction storing deltas
    // [10,20,30,40] for the real row [10,30,60,100].
    let mut pred = Vec::new();
    pred.extend_from_slice(&3u16.to_be_bytes());
    pred.extend_from_slice(&hex("78dae31291d3000000cc0065"));
    let raw = |fill: u8| {
        let mut v = Vec::new();
        v.extend_from_slice(&0u16.to_be_bytes());
        v.extend(std::iter::repeat_n(fill, 4));
        v
    };
    let mut rec = B {
        buf: Vec::new(),
        psb: false,
    };
    rec.buf.extend_from_slice(&1i32.to_be_bytes());
    record(
        &mut rec,
        "Pred",
        (0, 0, 1, 4),
        &[
            (-1, raw(255).len()),
            (0, pred.len()),
            (1, raw(0).len()),
            (2, raw(0).len()),
        ],
        b"norm",
        255,
    );
    let mut data = rec.buf;
    data.extend_from_slice(&raw(255));
    data.extend_from_slice(&pred);
    data.extend_from_slice(&raw(0));
    data.extend_from_slice(&raw(0));
    let bytes = B::new(4, 1, 8, 3, 3, &[], false).finish(data);
    let (doc, _) = pictodesk_core::psd::load_psd(&bytes).expect("pred psd");
    let got: Vec<u8> = (0..4)
        .map(|x| doc.layers[0].get_pixel(x, 0).to_rgba8().0)
        .collect();
    assert_eq!(got, vec![10, 30, 60, 100]);
}

#[test]
fn psd_16bit_layers() {
    // 4x1, 16-bit: R samples 1000, 2000, 30000, 65535.
    let raw16 = |vals: &[u16]| {
        let mut v = Vec::new();
        v.extend_from_slice(&0u16.to_be_bytes());
        for x in vals {
            v.extend_from_slice(&x.to_be_bytes());
        }
        v
    };
    let alpha = raw16(&[65535; 4]);
    let r = raw16(&[1000, 2000, 30000, 65535]);
    let zero = raw16(&[0; 4]);
    let mut rec = B {
        buf: Vec::new(),
        psb: false,
    };
    rec.buf.extend_from_slice(&1i32.to_be_bytes());
    record(
        &mut rec,
        "Deep",
        (0, 0, 1, 4),
        &[
            (-1, alpha.len()),
            (0, r.len()),
            (1, zero.len()),
            (2, zero.len()),
        ],
        b"norm",
        255,
    );
    let mut data = rec.buf;
    for c in [&alpha, &r, &zero, &zero] {
        data.extend_from_slice(c);
    }
    let bytes = B::new(4, 1, 16, 3, 3, &[], false).finish(data);
    let (doc, _) = pictodesk_core::psd::load_psd(&bytes).expect("16-bit psd");
    assert_eq!(doc.layers[0].get_pixel(0, 0).r, 1000);
    assert_eq!(doc.layers[0].get_pixel(2, 0).r, 30000);
    assert_eq!(doc.layers[0].get_pixel(3, 0).r, 65535);
}

#[test]
fn psd_indexed_and_mask_and_psb() {
    // Indexed 4x1 merged image: palette entry 1 = (10, 20, 30).
    let mut palette = vec![0u8; 768];
    palette[1] = 10;
    palette[256 + 1] = 20;
    palette[512 + 1] = 30;
    let mut merged = B::new(4, 1, 8, 2, 1, &palette, false);
    // No layer section (length 0), then merged data: raw, plane [0,1,1,0].
    merged.len64(0);
    merged.buf.extend_from_slice(&0u16.to_be_bytes());
    merged.buf.extend_from_slice(&[0, 1, 1, 0]);
    let (doc, _) = pictodesk_core::psd::load_psd(&merged.buf).expect("indexed");
    assert_eq!(doc.layers[0].get_pixel(1, 0).to_rgba8(), (10, 20, 30, 255));
    assert_eq!(doc.layers[0].get_pixel(0, 0).to_rgba8(), (0, 0, 0, 255));

    // Layer mask: red layer, mask ZIP plane of 128s -> alpha ~50%,
    // and the mask is preserved as an alpha channel.
    let raw = |fill: u8, n: usize| {
        let mut v = Vec::new();
        v.extend_from_slice(&0u16.to_be_bytes());
        v.extend(std::iter::repeat_n(fill, n));
        v
    };
    let mask = zip_chan(&hex("78da6b6880000012080401"));
    let mut rec = B {
        buf: Vec::new(),
        psb: false,
    };
    rec.buf.extend_from_slice(&1i32.to_be_bytes());
    record(
        &mut rec,
        "Masked",
        (0, 0, 2, 4),
        &[
            (-1, raw(255, 8).len()),
            (0, raw(255, 8).len()),
            (1, raw(0, 8).len()),
            (2, raw(0, 8).len()),
            (-2, mask.len()),
        ],
        b"norm",
        255,
    );
    let mut data = rec.buf;
    data.extend_from_slice(&raw(255, 8));
    data.extend_from_slice(&raw(255, 8));
    data.extend_from_slice(&raw(0, 8));
    data.extend_from_slice(&raw(0, 8));
    data.extend_from_slice(&mask);
    let bytes = B::new(4, 2, 8, 3, 3, &[], false).finish(data);
    let (doc, report) = pictodesk_core::psd::load_psd(&bytes).expect("mask psd");
    let a = doc.layers[0].get_pixel(1, 1).to_rgba8().3;
    assert!((120..=135).contains(&a), "mask should halve alpha, got {a}");
    assert!(doc.alphas.iter().any(|c| c.name == "Masked Mask"));
    assert!(report.notes.iter().any(|n| n.contains("layer mask")));

    // PSB: same two-layer file as the basic test, 64-bit lengths.
    let mut rec = B {
        buf: Vec::new(),
        psb: true,
    };
    rec.buf.extend_from_slice(&1i32.to_be_bytes());
    record(
        &mut rec,
        "Base",
        (0, 0, 2, 4),
        &[
            (-1, raw(255, 8).len()),
            (0, raw(255, 8).len()),
            (1, raw(0, 8).len()),
            (2, raw(0, 8).len()),
        ],
        b"norm",
        255,
    );
    let mut data = rec.buf;
    data.extend_from_slice(&raw(255, 8));
    data.extend_from_slice(&raw(255, 8));
    data.extend_from_slice(&raw(0, 8));
    data.extend_from_slice(&raw(0, 8));
    let bytes = B::new(4, 2, 8, 3, 3, &[], true).finish(data);
    let (doc, report) = pictodesk_core::psd::load_psd(&bytes).expect("psb");
    assert_eq!(report.layers_imported, 1);
    assert_eq!(doc.layers[0].get_pixel(2, 1).to_rgba8(), (255, 0, 0, 255));
}
