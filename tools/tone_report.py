import argparse
import os
import re
from dataclasses import dataclass
from typing import Dict, Iterable, List, Optional, Tuple


def read_text(path: str) -> str:
    raw = open(path, "rb").read()
    if raw.startswith(b"\xff\xfe") or raw[:200].count(b"\x00") > 20:
        if raw.startswith(b"\xff\xfe"):
            raw = raw[2:]
        return raw.decode("utf-16le", errors="ignore")
    return raw.decode("utf-8", errors="ignore")


PARAM_LINE = re.compile(r"^\s*(\d+)\s+(.+?)\s+=\s+([0-9.]+)\s*$")


@dataclass(frozen=True)
class ParsedLog:
    filename: str
    reasoning: str
    warnings: List[str]
    model_params: List[Tuple[int, str, float, str]]  # idx, label, value, group
    added_params: List[Tuple[int, str, float, str]]


@dataclass(frozen=True)
class PromptSpec:
    prompt: str
    allow_delay: bool = False
    allow_reverb: bool = False
    require_delay: bool = False
    require_chorus: bool = False


def parse_log(path: str) -> ParsedLog:
    txt = read_text(path)
    filename = os.path.basename(path)

    reasoning = ""
    m = re.search(r"\breasoning:\s*\n(.*?)\n\s*qc:\s*\n", txt, re.S | re.I)
    if m:
        reasoning = m.group(1).strip()

    warnings: List[str] = []
    in_warn = False
    for line in txt.splitlines():
        s = line.strip()
        if s == "warnings:":
            in_warn = True
            continue
        if in_warn and s.startswith("- "):
            warnings.append(s[2:])
            continue
        if in_warn and s.startswith("preview_only="):
            break

    model_params: List[Tuple[int, str, float, str]] = []
    added_params: List[Tuple[int, str, float, str]] = []
    cur: Optional[str] = None
    group: Optional[str] = None
    for line in txt.splitlines():
        s = line.strip()
        if s == "model (sanitized):":
            cur = "model"
            group = None
            continue
        if s == "added_by_replace_active:":
            cur = "added"
            group = None
            continue
        g = re.match(r"^\s*\[(.+?)\]\s*$", line)
        if g and cur:
            group = g.group(1)
            continue
        m2 = PARAM_LINE.match(line)
        if m2 and cur and group:
            idx = int(m2.group(1))
            label = m2.group(2).strip()
            val = float(m2.group(3))
            if cur == "model":
                model_params.append((idx, label, val, group))
            else:
                added_params.append((idx, label, val, group))

    return ParsedLog(
        filename=filename,
        reasoning=reasoning,
        warnings=warnings,
        model_params=model_params,
        added_params=added_params,
    )


def build_prompt_map() -> Dict[str, PromptSpec]:
    return {
        "metallica_black_1991": PromptSpec(
            prompt="James Hetfield rhythm tone - Metallica, Black Album (1991). Tight but not overly scooped, thick low mids, smooth top end, minimal fizz. No delay. Very subtle room reverb.",
            allow_reverb=True,
        ),
        "metallica_mop_1986": PromptSpec(
            prompt="Metallica rhythm tone - Master of Puppets (1986). Aggressive upper mids, crunchy, less sub bass, slightly rawer top. No delay. Almost no reverb.",
            allow_reverb=True,
        ),
        "slayer_reign_1986": PromptSpec(
            prompt="Slayer rhythm tone - Reign in Blood (1986). Fast tight palm-mutes, biting upper mids, gritty saturation, controlled low end. No delay. No reverb.",
        ),
        "pantera_cfh_1990": PromptSpec(
            prompt="Pantera rhythm tone - Cowboys From Hell (1990). Bright aggressive pick attack, tight low end, pronounced presence, not fizzy. No delay. Minimal reverb.",
            allow_reverb=True,
        ),
        "sepultura_roots_1996": PromptSpec(
            prompt="Sepultura rhythm tone - Roots (1996). Mid-forward chunky groove, thick but controlled, less polished, controlled highs. No delay. Very subtle room reverb.",
            allow_reverb=True,
        ),
        "gojira_fm2s_2005": PromptSpec(
            prompt="Gojira rhythm tone - From Mars to Sirius (2005). Very tight low end, pronounced pick attack, modern but organic, controlled fizz. No delay. Subtle short room reverb.",
            allow_reverb=True,
        ),
        "meshuggah_obzen_2008": PromptSpec(
            prompt="Meshuggah rhythm tone - obZen (2008) 8-string. Extremely tight low end, focused low mids, crisp attack, minimal hiss. No delay. No reverb.",
        ),
        "lambofgod_ashes_2004": PromptSpec(
            prompt="Lamb of God rhythm tone - Ashes of the Wake (2004). Punchy midrange, tight palm-mutes, aggressive but not fizzy. No delay. Tiny room reverb.",
            allow_reverb=True,
        ),
        "killswitch_eoh_2004": PromptSpec(
            prompt="Killswitch Engage rhythm tone - The End of Heartache (2004). Polished modern metalcore: tight, slightly scooped but present mids, bright but smooth. No delay. Subtle reverb.",
            allow_reverb=True,
        ),
        "inflames_clayman_2000": PromptSpec(
            prompt="In Flames rhythm tone - Clayman (2000). Swedish melodeath bite, strong upper mids, tight low end, controlled top end. No delay. Minimal reverb.",
            allow_reverb=True,
        ),
        "trivium_shogun_2008": PromptSpec(
            prompt="Trivium rhythm tone - Shogun (2008). Tight and saturated, defined low mids, clear pick attack, not harsh. No delay. Subtle room reverb.",
            allow_reverb=True,
        ),
        "periphery_p2_2012": PromptSpec(
            prompt="Periphery rhythm tone - Periphery II (2012). Modern djent: very tight low end, crisp attack, controlled fizz; slightly scooped but articulate. No delay. No reverb.",
        ),
        "opeth_bwp_2001": PromptSpec(
            prompt="Opeth heavy rhythm tone - Blackwater Park (2001). Thick low mids, darker top end, organic saturation, not ultra-tight modern. No delay. Small room reverb very low.",
            allow_reverb=True,
        ),
        "maiden_sit_lead_1986": PromptSpec(
            prompt="Iron Maiden lead tone - Somewhere in Time era (1986). Singing sustain, bright but smooth, stereo-ish space. Use delay (audible but not overpowering) and a touch of reverb.",
            allow_delay=True,
            allow_reverb=True,
            require_delay=True,
        ),
        "metallica_nem_clean_1991": PromptSpec(
            prompt="Metallica clean tone - Nothing Else Matters (1991). Sparkly but warm clean, light chorus, subtle reverb, no delay.",
            allow_reverb=True,
            require_chorus=True,
        ),
    }


def amp_name(v: float) -> str:
    if abs(v - 0.0) < 0.2:
        return "Clean"
    if abs(v - 0.5) < 0.2:
        return "Rust"
    if abs(v - 1.0) < 0.2:
        return "Hot"
    return f"Custom({v:.3f})"


def is_on(v: Optional[float]) -> bool:
    return v is not None and v >= 0.5


def first_val(m: Dict[int, float], idx: int) -> Optional[float]:
    return m.get(idx)


def format_param(idx: int, label: str, val: float) -> str:
    return f"- `{idx}` {label}: `{val:.3f}`"


def touched_indices(params: Iterable[Tuple[int, str, float, str]]) -> List[int]:
    return sorted({idx for (idx, _, _, _) in params})


def any_in_range(idxs: Iterable[int], start: int, end: int) -> bool:
    for i in idxs:
        if start <= i <= end:
            return True
    return False


def values_in_range(m: Dict[int, float], start: int, end: int) -> Dict[int, float]:
    return {k: v for (k, v) in m.items() if start <= k <= end}


def detect_logic_flags(stem: str, spec: PromptSpec, item: ParsedLog) -> List[str]:
    model_map: Dict[int, float] = {idx: val for (idx, _, val, _) in item.model_params}
    idxs = set(model_map.keys())
    flags: List[str] = []

    amp_val = first_val(model_map, 29)
    amp = amp_name(amp_val) if amp_val is not None else "Unset"

    # Cross-amp controls
    if amp == "Clean":
        if any_in_range(idxs, 36, 51):
            flags.append("amp_type=Clean but Rust/Hot amp controls touched (36-51)")
        if any_in_range(idxs, 63, 82):
            flags.append("amp_type=Clean but Rust/Hot EQ controls touched (63-82)")
    elif amp == "Rust":
        if any_in_range(idxs, 30, 35) or any_in_range(idxs, 44, 51):
            flags.append("amp_type=Rust but Clean/Hot amp controls touched (30-35 or 44-51)")
        if any_in_range(idxs, 53, 62) or any_in_range(idxs, 73, 82):
            flags.append("amp_type=Rust but Clean/Hot EQ controls touched (53-62 or 73-82)")
    elif amp == "Hot":
        if any_in_range(idxs, 30, 43):
            flags.append("amp_type=Hot but Clean/Rust amp controls touched (30-43)")
        if any_in_range(idxs, 53, 72):
            flags.append("amp_type=Hot but Clean/Rust EQ controls touched (53-72)")

    # Module toggle consistency
    # OD
    if any_in_range(idxs, 14, 16) and not is_on(first_val(model_map, 13)):
        flags.append("OD params set (14-16) but OD Active (13) missing/off")
    # DRT
    if any_in_range(idxs, 18, 20) and not is_on(first_val(model_map, 17)):
        flags.append("DRT params set (18-20) but DRT Active (17) missing/off")
    # Chorus
    if any_in_range(idxs, 24, 27) and not is_on(first_val(model_map, 23)):
        flags.append("Chorus params set (24-27) but CHR Active (23) missing/off")
    # Delay
    delay_touched = any_in_range(idxs, 102, 111) or (105 in idxs) or (106 in idxs) or (108 in idxs) or (110 in idxs)
    if delay_touched and not is_on(first_val(model_map, 101)):
        flags.append("Delay params set (>=102) but DLY Active (101) missing/off")
    # Reverb
    reverb_touched = any_in_range(idxs, 113, 117) or (114 in idxs)
    if reverb_touched and not is_on(first_val(model_map, 112)):
        flags.append("Reverb params set (>=113) but REV Active (112) missing/off")
    # Cab
    cab_touched = any_in_range(idxs, 84, 99)  # exclude 100: FX Section Active
    if cab_touched and not is_on(first_val(model_map, 83)):
        flags.append("Cab params set (84-99) but Cab Section Active (83) missing/off")
    if (86 in idxs or 93 in idxs) and not is_on(first_val(model_map, 83)):
        flags.append("Cab active toggles set (86/93) but Cab Section Active (83) missing/off")

    # Prompt expectations
    if is_on(first_val(model_map, 101)) and not spec.allow_delay:
        flags.append("Delay is ON (101) but prompt says no delay")
    if not is_on(first_val(model_map, 101)) and spec.require_delay:
        flags.append("Delay is OFF/missing (101) but prompt requires delay")
    if is_on(first_val(model_map, 112)) and not spec.allow_reverb:
        flags.append("Reverb is ON (112) but prompt says no reverb")
    if is_on(first_val(model_map, 23)) is False and spec.require_chorus:
        flags.append("Chorus is OFF/missing (23) but prompt requires chorus")

    # Reasoning red flags (LLM output hygiene)
    rlow = item.reasoning.lower()
    if "compressor" in rlow:
        flags.append("Reasoning mentions compressor (plugin has no dedicated compressor)")
    if re.search(r"\b\d+(\.\d+)?\s*(hz|khz)\b", item.reasoning, flags=re.I):
        flags.append("Reasoning mentions Hz/kHz (prompt asked to avoid)")

    # Reasoning vs params contradictions (light heuristics)
    if "delay is turned off" in rlow or "delay turned off" in rlow:
        if is_on(first_val(model_map, 101)):
            flags.append('Reasoning says "delay off" but DLY Active (101) is ON')
    if "no reverb" in rlow:
        if is_on(first_val(model_map, 112)):
            flags.append('Reasoning says "no reverb" but REV Active (112) is ON')

    # Special: verify that delay mix (105) and reverb mix (114) are used when those effects are on
    if is_on(first_val(model_map, 101)) and 105 not in idxs:
        flags.append("Delay is ON (101) but DLY Dry/Wet (105) not set by model")
    if is_on(first_val(model_map, 112)) and 114 not in idxs:
        flags.append("Reverb is ON (112) but REV Dry/Wet (114) not set by model")

    return flags


def extract_key_params(item: ParsedLog) -> List[str]:
    model_map: Dict[int, float] = {idx: val for (idx, _, val, _) in item.model_params}
    amp_val = first_val(model_map, 29)
    amp = amp_name(amp_val) if amp_val is not None else "Unset"

    keys: List[Tuple[int, str]] = [(29, "Amp Type"), (2, "Gate Amount")]
    if amp == "Clean":
        keys += [
            (30, "CLN Gain"),
            (31, "CLN Bright"),
            (32, "CLN Bass"),
            (33, "CLN Mid"),
            (34, "CLN Treble"),
            (35, "CLN Level"),
            (53, "CLN EQ Active"),
        ]
    elif amp == "Rust":
        keys += [
            (36, "RUST Gain"),
            (37, "RUST Low"),
            (38, "RUST Mid"),
            (39, "RUST High"),
            (40, "RUST Master"),
            (41, "RUST Presence"),
            (42, "RUST Depth"),
            (43, "RUST Level"),
            (63, "RUST EQ Active"),
        ]
    elif amp == "Hot":
        keys += [
            (44, "HOT Gain"),
            (45, "HOT Low"),
            (46, "HOT Mid"),
            (47, "HOT High"),
            (48, "HOT Master"),
            (49, "HOT Presence"),
            (50, "HOT Depth"),
            (51, "HOT Level"),
            (73, "HOT EQ Active"),
        ]

    keys += [
        (52, "EQ Section Active"),
        (13, "OD Active"),
        (17, "DRT Active"),
        (23, "CHR Active"),
        (101, "DLY Active"),
        (105, "DLY Dry/Wet"),
        (106, "DLY Feedback"),
        (112, "REV Active"),
        (114, "REV Dry/Wet"),
        (83, "Cab Section Active"),
        (86, "Cab 1 Active"),
        (93, "Cab 2 Active"),
    ]

    out: List[str] = []
    for idx, name in keys:
        v = first_val(model_map, idx)
        if v is None:
            continue
        out.append(f"- {name} ({idx}): `{v:.3f}`")
    return out


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--dir", required=True, help="Directory containing tone .log files")
    ap.add_argument("--out", required=True, help="Output markdown report path")
    args = ap.parse_args()

    prompt_map = build_prompt_map()
    logs = sorted([p for p in os.listdir(args.dir) if p.lower().endswith(".log")])
    parsed: List[ParsedLog] = [parse_log(os.path.join(args.dir, f)) for f in logs]

    # Aggregate warnings (existing + derived)
    warning_counts: Dict[str, int] = {}
    derived_counts: Dict[str, int] = {}

    derived_by_file: Dict[str, List[str]] = {}
    for item in parsed:
        for w in item.warnings:
            warning_counts[w] = warning_counts.get(w, 0) + 1

        stem = re.sub(r"^\d+_", "", item.filename)
        stem = re.sub(r"\.log$", "", stem, flags=re.I)
        spec = prompt_map.get(stem) or PromptSpec(prompt="(prompt not recorded)")
        derived = detect_logic_flags(stem, spec, item)
        derived_by_file[item.filename] = derived
        for w in derived:
            derived_counts[w] = derived_counts.get(w, 0) + 1

    lines: List[str] = []
    lines.append(f"# Tone Engineering Report ({os.path.basename(args.dir)})")
    lines.append("")
    lines.append(f"Tones: **{len(parsed)}**")
    lines.append("")

    if warning_counts:
        lines.append("## CLI QC Warning Summary")
        for w, c in sorted(warning_counts.items(), key=lambda kv: (-kv[1], kv[0])):
            lines.append(f"- `{c}`x {w}")
        lines.append("")

    if derived_counts:
        lines.append("## Derived QA Flag Summary")
        for w, c in sorted(derived_counts.items(), key=lambda kv: (-kv[1], kv[0])):
            lines.append(f"- `{c}`x {w}")
        lines.append("")

    for item in parsed:
        base = item.filename
        stem = re.sub(r"^\d+_", "", base)
        stem = re.sub(r"\.log$", "", stem, flags=re.I)
        spec = prompt_map.get(stem) or PromptSpec(prompt="(prompt not recorded)")

        model_map = {idx: val for (idx, _, val, _) in item.model_params}
        amp_val = model_map.get(29)
        amp = amp_name(amp_val) if amp_val is not None else "Unset"

        lines.append(f"## {base}")
        lines.append(f"- Prompt: {spec.prompt}")
        lines.append(f"- Amp Type (29): **{amp}**" + (f" (`{amp_val:.3f}`)" if amp_val is not None else ""))
        lines.append("")

        keys = extract_key_params(item)
        if keys:
            lines.append("### Key Params")
            lines.extend(keys)
            lines.append("")

        derived = derived_by_file.get(base, [])
        if derived:
            lines.append("### Derived QA Flags")
            for w in derived:
                lines.append(f"- {w}")
            lines.append("")

        lines.append("### Model Params")
        by_group: Dict[str, List[Tuple[int, str, float]]] = {}
        for idx, label, val, grp in item.model_params:
            by_group.setdefault(grp, []).append((idx, label, val))
        for grp in sorted(by_group.keys()):
            lines.append(f"**{grp}**")
            for idx, label, val in sorted(by_group[grp], key=lambda t: t[0]):
                lines.append(format_param(idx, label, val))
        lines.append("")

        if item.added_params:
            lines.append("### Added By ReplaceActive")
            by_group2: Dict[str, List[Tuple[int, str, float]]] = {}
            for idx, label, val, grp in item.added_params:
                by_group2.setdefault(grp, []).append((idx, label, val))
            for grp in sorted(by_group2.keys()):
                lines.append(f"**{grp}**")
                for idx, label, val in sorted(by_group2[grp], key=lambda t: t[0]):
                    lines.append(format_param(idx, label, val))
            lines.append("")

        lines.append("### Reasoning")
        lines.append(item.reasoning if item.reasoning else "(missing)")
        lines.append("")

    os.makedirs(os.path.dirname(args.out) or ".", exist_ok=True)
    with open(args.out, "w", encoding="utf-8") as f:
        f.write("\n".join(lines).rstrip() + "\n")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
