"use client";

import { useState } from "react";
import { WaveLine } from "../app/wave-line";

const GITHUB_URL = "https://github.com/fantasycorp/superb";
const INSTALL = "cargo add superb";

// The quick start: a session, a track, a buffer, then audio back with no
// device. The assert is what gets copied; the page shows it as the result.
const PROGRAM = `use superb::primitives::buffers::PlanarBuffer;
use superb::primitives::session::TrackKind;
use superb::primitives::{SampleRate, TimePosition};
use superb::{clip, session, track, transport};

let mut session = session::create("demo.superb", SampleRate::HZ_48K, 256)?;
let lead = track::add(&mut session, "lead", TrackKind::Audio)?;

let mut tone = PlanarBuffer::new(2, 48_000);
tone.fill(0.25);
session.audio.insert("tone.wav", tone);
clip::add(&mut session, lead, "tone.wav", TimePosition::ZERO)?;
track::volume(&mut session, lead, -6.0)?;

transport::start(&mut session)?;
let audio = session::steps::render_blocks(&mut session, 8)?;
assert!(audio.peak() > 0.1);`;

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
                  <Tok key="k1" kind="kw">use</Tok>,
                  " superb::primitives::buffers::",
                  <Tok key="t1" kind="ty">PlanarBuffer</Tok>,
                  ";\n",
                  <Tok key="k2" kind="kw">use</Tok>,
                  " superb::primitives::session::",
                  <Tok key="t2" kind="ty">TrackKind</Tok>,
                  ";\n",
                  <Tok key="k3" kind="kw">use</Tok>,
                  " superb::primitives::{",
                  <Tok key="t3" kind="ty">SampleRate</Tok>,
                  ", ",
                  <Tok key="t4" kind="ty">TimePosition</Tok>,
                  "};\n",
                  <Tok key="k4" kind="kw">use</Tok>,
                  " superb::{clip, session, track, transport};\n\n",
                  <Tok key="k5" kind="kw">let</Tok>,
                  " ",
                  <Tok key="k6" kind="kw">mut</Tok>,
                  " session = session::",
                  <Tok key="f1" kind="fn">create</Tok>,
                  "(",
                  <Tok key="s1" kind="str">&quot;demo.superb&quot;</Tok>,
                  ", ",
                  <Tok key="t5" kind="ty">SampleRate</Tok>,
                  "::",
                  <Tok key="c1" kind="const">HZ_48K</Tok>,
                  ", ",
                  <Tok key="n1" kind="num">256</Tok>,
                  ")?;\n",
                  <Tok key="k7" kind="kw">let</Tok>,
                  " lead = track::",
                  <Tok key="f2" kind="fn">add</Tok>,
                  "(",
                  <Tok key="op1" kind="op">&amp;mut</Tok>,
                  " session, ",
                  <Tok key="s2" kind="str">&quot;lead&quot;</Tok>,
                  ", ",
                  <Tok key="t6" kind="ty">TrackKind</Tok>,
                  "::",
                  <Tok key="c2" kind="const">Audio</Tok>,
                  ")?;\n\n",
                  <Tok key="k8" kind="kw">let</Tok>,
                  " ",
                  <Tok key="k9" kind="kw">mut</Tok>,
                  " tone = ",
                  <Tok key="t7" kind="ty">PlanarBuffer</Tok>,
                  "::",
                  <Tok key="f3" kind="fn">new</Tok>,
                  "(",
                  <Tok key="n2" kind="num">2</Tok>,
                  ", ",
                  <Tok key="n3" kind="num">48_000</Tok>,
                  ");\n",
                  "tone.",
                  <Tok key="f4" kind="fn">fill</Tok>,
                  "(",
                  <Tok key="n4" kind="num">0.25</Tok>,
                  ");\n",
                  "session.audio.",
                  <Tok key="f5" kind="fn">insert</Tok>,
                  "(",
                  <Tok key="s3" kind="str">&quot;tone.wav&quot;</Tok>,
                  ", tone);\n",
                  "clip::",
                  <Tok key="f6" kind="fn">add</Tok>,
                  "(",
                  <Tok key="op2" kind="op">&amp;mut</Tok>,
                  " session, lead, ",
                  <Tok key="s4" kind="str">&quot;tone.wav&quot;</Tok>,
                  ", ",
                  <Tok key="t8" kind="ty">TimePosition</Tok>,
                  "::",
                  <Tok key="c3" kind="const">ZERO</Tok>,
                  ")?;\n",
                  "track::",
                  <Tok key="f7" kind="fn">volume</Tok>,
                  "(",
                  <Tok key="op3" kind="op">&amp;mut</Tok>,
                  " session, lead, ",
                  <Tok key="n5" kind="num">-6.0</Tok>,
                  ")?;\n\n",
                  "transport::",
                  <Tok key="f8" kind="fn">start</Tok>,
                  "(",
                  <Tok key="op4" kind="op">&amp;mut</Tok>,
                  " session)?;\n",
                  <Tok key="k10" kind="kw">let</Tok>,
                  " audio = session::steps::",
                  <Tok key="f9" kind="fn">render_blocks</Tok>,
                  "(",
                  <Tok key="op5" kind="op">&amp;mut</Tok>,
                  " session, ",
                  <Tok key="n6" kind="num">8</Tok>,
                  ")?;\n\n",
                  <Tok key="m1" kind="mac">assert!</Tok>,
                  "(audio.",
                  <Tok key="f10" kind="fn">peak</Tok>,
                  "() > ",
                  <Tok key="n7" kind="num">0.1</Tok>,
                  ");\n",
                  <span key="mark" className="answerMark">&gt;</span>,
                  " ",
                  <span key="answer" className="answer">peak &gt; 0.1</span>,
                  "\n  ",
                  <span key="meta" className="meta">8 blocks, no sound card</span>,
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
