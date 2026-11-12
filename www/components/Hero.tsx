"use client";

import { useState } from "react";
import { WaveLine } from "../app/wave-line";

const GITHUB_URL = "https://github.com/fantasycorp/superb";
const INSTALL = "cargo add superb --features devices";

// The quick start: make a sound, play it through the default output, bounce
// it to a WAV. PROGRAM is what gets copied, a whole program; the page shows
// its body.
const PROGRAM = `use std::{thread, time::Duration};
use superb::export::{self, Bounce};
use superb::primitives::buffers::PlanarBuffer;
use superb::primitives::session::TrackKind;
use superb::primitives::{Result, SampleRate, TimePosition};
use superb::{clip, devices, session, track, transport};

fn main() -> Result<()> {
    let mut session = session::create("demo", SampleRate::HZ_48K, 256)?;
    let lead = track::add(&mut session, "lead", TrackKind::Audio)?;

    // Create one second of audio
    let tone = PlanarBuffer::sine(440.0, 0.25, SampleRate::HZ_48K, 48_000);
    session.audio.insert("audio/tone.wav", tone);
    clip::add(&mut session, lead, "audio/tone.wav", TimePosition::ZERO)?;

    // Plays audio
    devices::output(&mut session, None)?;
    transport::start(&mut session)?;
    // Waiting until audio is finished playing to bounce
    thread::sleep(Duration::from_secs(1));

    // Bounce as a .wav
    export::bounce(&mut session, "demo/bounces/mix.wav", Bounce::default())?;
    Ok(())
}`;

function CopyButton({ value, label }: { value: string; label: string }) {
  const [copied, setCopied] = useState(false);

  const copy = async () => {
    try {
      await navigator.clipboard.writeText(value);
      setCopied(true);
      window.setTimeout(() => setCopied(false), 1400);
    } catch {
      // clipboard is unavailable outside a secure context
    }
  };

  return (
    <button
      type="button"
      className="copy"
      onClick={copy}
      aria-label={label}
      title={copied ? "Copied" : label}
    >
      {copied ? (
        <svg viewBox="0 0 24 24" fill="none" aria-hidden="true">
          <path
            d="M20 6L9 17l-5-5"
            stroke="currentColor"
            strokeWidth="2"
            strokeLinecap="round"
            strokeLinejoin="round"
          />
        </svg>
      ) : (
        <svg viewBox="0 0 24 24" fill="none" aria-hidden="true">
          <rect
            x="8"
            y="8"
            width="14"
            height="14"
            rx="2"
            stroke="currentColor"
            strokeWidth="2"
          />
          <path
            d="M4 16c-1.1 0-2-.9-2-2V4c0-1.1.9-2 2-2h10c1.1 0 2 .9 2 2"
            stroke="currentColor"
            strokeWidth="2"
            strokeLinecap="round"
            strokeLinejoin="round"
          />
        </svg>
      )}
    </button>
  );
}

function Tok({
  kind,
  children,
}: {
  kind: "kw" | "fn" | "ty" | "const" | "num" | "mac" | "str" | "op";
  children: string;
}) {
  return <span className={`tok tok-${kind}`}>{children}</span>;
}

export function Hero() {
  return (
    <main className="page">
      <div className="stack">
        <h1 className="wordmark">SUPERB</h1>
        <p className="tagline">
          A fully-featured audio SDK written in Rust.
          <br />
          Create your own DAW in a few lines of code.
        </p>

          <div className="block">
            <code className="line">
              <span className="prompt">$</span> {INSTALL}
            </code>
            <CopyButton value={INSTALL} label="Copy install command" />
          </div>

          <div className="block result">
            <pre className="pre">
              <code>
                {[
                  <Tok key="x1" kind="kw">let</Tok>,
                  " ",
                  <Tok key="x2" kind="kw">mut</Tok>,
                  " session = session::",
                  <Tok key="x3" kind="fn">create</Tok>,
                  "(",
                  <Tok key="x4" kind="str">&quot;demo&quot;</Tok>,
                  ", ",
                  <Tok key="x5" kind="ty">SampleRate</Tok>,
                  "::",
                  <Tok key="x6" kind="const">HZ_48K</Tok>,
                  ", ",
                  <Tok key="x7" kind="num">256</Tok>,
                  ")?;\n",
                  <Tok key="x8" kind="kw">let</Tok>,
                  " lead = track::",
                  <Tok key="x9" kind="fn">add</Tok>,
                  "(",
                  <Tok key="x10" kind="op">&amp;mut</Tok>,
                  " session, ",
                  <Tok key="x11" kind="str">&quot;lead&quot;</Tok>,
                  ", ",
                  <Tok key="x12" kind="ty">TrackKind</Tok>,
                  "::",
                  <Tok key="x13" kind="const">Audio</Tok>,
                  ")?;\n\n",
                  <span key="x14" className="meta">{"// Create one second of audio"}</span>,
                  "\n",
                  <Tok key="x15" kind="kw">let</Tok>,
                  " tone = ",
                  <Tok key="x16" kind="ty">PlanarBuffer</Tok>,
                  "::",
                  <Tok key="x17" kind="fn">sine</Tok>,
                  "(",
                  <Tok key="x18" kind="num">440.0</Tok>,
                  ", ",
                  <Tok key="x19" kind="num">0.25</Tok>,
                  ", ",
                  <Tok key="x20" kind="ty">SampleRate</Tok>,
                  "::",
                  <Tok key="x21" kind="const">HZ_48K</Tok>,
                  ", ",
                  <Tok key="x22" kind="num">48_000</Tok>,
                  ");\nsession.audio.",
                  <Tok key="x23" kind="fn">insert</Tok>,
                  "(",
                  <Tok key="x24" kind="str">&quot;audio/tone.wav&quot;</Tok>,
                  ", tone);\nclip::",
                  <Tok key="x25" kind="fn">add</Tok>,
                  "(",
                  <Tok key="x26" kind="op">&amp;mut</Tok>,
                  " session, lead, ",
                  <Tok key="x27" kind="str">&quot;audio/tone.wav&quot;</Tok>,
                  ", ",
                  <Tok key="x28" kind="ty">TimePosition</Tok>,
                  "::",
                  <Tok key="x29" kind="const">ZERO</Tok>,
                  ")?;\n\n",
                  <span key="x30" className="meta">{"// Plays audio"}</span>,
                  "\ndevices::",
                  <Tok key="x31" kind="fn">output</Tok>,
                  "(",
                  <Tok key="x32" kind="op">&amp;mut</Tok>,
                  " session, ",
                  <Tok key="x33" kind="const">None</Tok>,
                  ")?;\ntransport::",
                  <Tok key="x34" kind="fn">start</Tok>,
                  "(",
                  <Tok key="x35" kind="op">&amp;mut</Tok>,
                  " session)?;\n",
                  <span key="x36" className="meta">{"// Waiting until audio is finished playing to bounce"}</span>,
                  "\nthread::",
                  <Tok key="x37" kind="fn">sleep</Tok>,
                  "(",
                  <Tok key="x38" kind="ty">Duration</Tok>,
                  "::",
                  <Tok key="x39" kind="fn">from_secs</Tok>,
                  "(",
                  <Tok key="x40" kind="num">1</Tok>,
                  "));\n\n",
                  <span key="x41" className="meta">{"// Bounce as a .wav"}</span>,
                  "\nexport::",
                  <Tok key="x42" kind="fn">bounce</Tok>,
                  "(",
                  <Tok key="x43" kind="op">&amp;mut</Tok>,
                  " session, ",
                  <Tok key="x44" kind="str">&quot;demo/bounces/mix.wav&quot;</Tok>,
                  ", ",
                  <Tok key="x45" kind="ty">Bounce</Tok>,
                  "::",
                  <Tok key="x46" kind="fn">default</Tok>,
                  "())?;",
                ]}
              </code>
            </pre>
            <CopyButton value={PROGRAM} label="Copy example" />
          </div>

          <div className="links">
            <a className="docs" href={GITHUB_URL}>
              <span className="docsLabel">
                Documentation <span className="arrow">↗</span>
              </span>
              <WaveLine className="docsWave" height={8} />
            </a>
            <a
              className="iconLink"
              href={GITHUB_URL}
              title="GitHub"
              aria-label="GitHub"
            >
              <span className="iconSpin">
                <svg viewBox="0 0 24 24" fill="currentColor" aria-hidden="true">
                  <path d="M12 .297c-6.63 0-12 5.373-12 12 0 5.303 3.438 9.8 8.205 11.385.6.113.82-.258.82-.577 0-.285-.01-1.04-.015-2.04-3.338.724-4.042-1.61-4.042-1.61C4.422 18.07 3.633 17.7 3.633 17.7c-1.087-.744.084-.729.084-.729 1.205.084 1.838 1.236 1.838 1.236 1.07 1.835 2.809 1.305 3.495.998.108-.776.417-1.305.76-1.605-2.665-.3-5.466-1.332-5.466-5.93 0-1.31.465-2.38 1.235-3.22-.135-.303-.54-1.523.105-3.176 0 0 1.005-.322 3.3 1.23.96-.267 1.98-.399 3-.405 1.02.006 2.04.138 3 .405 2.28-1.552 3.285-1.23 3.285-1.23.645 1.653.24 2.873.12 3.176.765.84 1.23 1.91 1.23 3.22 0 4.61-2.805 5.625-5.475 5.92.42.36.81 1.096.81 2.22 0 1.606-.015 2.896-.015 3.286 0 .315.21.69.825.57C20.565 22.092 24 17.592 24 12.297c0-6.627-5.373-12-12-12" />
                </svg>
              </span>
            </a>
          </div>
      </div>

      <p className="copyright">© 2026 *^* FANTASY</p>
    </main>
  );
}
