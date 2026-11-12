#!/usr/bin/env python3
"""Review my private memory for things that can go public, only when dhilipsiva asks.

    public-review.py build   --run DIR    inventory, scan and write DIR/review.md for him to prune
    public-review.py kept    --run DIR    list the items still in DIR/review.md
    public-review.py publish --run DIR [--dry-run]
                                          publish only the kept items, through the lucy CLI

The public-review skill (lucy-plugin/skills/public-review) says when to run each step.
Nothing here runs on its own, and nothing is published before he has pruned the review.
A build looks only at records newer than the last published review (STATE below), so
what he kept private once is not offered again; --since overrides that.

Private inputs never enter a repository. The run directory holds DIR/plan.json, my
judgement for this review (what each private conversation is about and what stays out),
and every output. The private settings file (--private, default
~/LucyTasks/public-review.private.json) holds the names and phrases that must never go
public; this script keeps only the generic patterns.
"""
import argparse, datetime, difflib, glob, hashlib, json, os, re, subprocess, sys

HOME = os.path.expanduser("~")
STORE = os.environ.get("LUCY_DATA_DIR", os.path.join(HOME, ".local/share/lucy"))
NOTES = os.path.join(HOME, ".local/state/lucy/notes")
PRIVATE = os.path.join(HOME, "LucyTasks/public-review.private.json")
STATE = os.environ.get("LUCY_REVIEW_STATE", os.path.join(HOME, "LucyTasks/public-review.state.json"))
ORIGIN = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
ARCHIVE = os.path.join(ORIGIN, "lucy", "interactions.nibli")
I = re.I

# Generic patterns. A hit keeps a record out of the public sections until I read it; no
# hit is not proof that a record is safe, which is why he prunes the review.
GENERIC = {
    "secret": r"ghp_[A-Za-z0-9]{20,}|github_pat_\w{20,}|gho_\w{20,}|\bsk-[A-Za-z0-9-]{20,}|AKIA[0-9A-Z]{16}"
              r"|xox[baprs]-\w+|-----BEGIN [A-Z ]*PRIVATE KEY|(?:password|passwd|api[_-]?key|secret|token)\s*[:=]\s*"
              r"['\"]?[A-Za-z0-9_\-]{16,}|Bearer\s+[A-Za-z0-9._\-]{20,}",
    "email": r"(?<!git)(?<![\w.+-])[\w.+-]+@(?!github\.com)(?!\d+\.service)[\w-]+\.[a-z]{2,}",
    "phone": r"(?<![\w.:/-])\+\d{1,3}[\s-]?\d{4,5}[\s-]?\d{4,6}(?![\w-])|(?<![\w.:/-])[6-9]\d{9}(?![\w-])",
    "children": r"\bmy (?:son|sons|daughter|daughters|kids?|children|boys|girls)\b|\byour (?:son|sons|daughter|kids|children)\b"
                r"|\bkids?'? ?(?:'s)? laptop|\bschool\b",
    "family": r"\bwife\b|\bhusband\b|\bcousin|grandfather|grandmother|\bmy (?:mother|father|mom|dad|parents|brother|sister)\b"
              r"|\bin-laws?\b|\bbrother in law\b|\brelatives\b",
    "jobsearch": r"recruit|interview|hiring|\bcareers?\b|job application|outreach|LinkedIn|r[ée]sum[ée]|\bCV\b|cover letter"
                 r"|relocat\w* to\b|compensation|salary|offer letter|\bvisa\b|sponsorship",
    "substance": r"\bweed\b|cannabis|marijuana|\bstoned\b|\bganja\b|when I (?:was|get) high|\bdrunk\b|\balcohol\b",
    "health": r"\b(?:my|your|his|her|a|our|the family) doctors?\b|hospital|\bmedic(?:ine|ation)|\bdiagnosed with\b|\billness|therapy|depress|anxiety|surgery|prescription",
    "finance": r"\binvoice|\bbank\b|\bIFSC\b|\bUPI\b|credit card|\bCTC\b|\bLPA\b|₹|\brupees?\b|\bGST\b|\btax(?:es)?\b|billing",
    "identity_docs": r"Aadhaar|passport|\bvoter\b|\bPAN card\b|driving licen",
    "google_data": r"Google (?:Contacts|Photos|Calendar|Keep|Tasks|Drive|Takeout|Account)|\bGmail\b|Gemini personali[sz]ation",
}
HARNESS = re.compile(r"^\s*(?:<task-notification>|<agent-message|<system-reminder>|<command-name>|<local-command"
                     r"|<environment_context>|\[Subagent hand-back\])", I)
PREFIX = {"claude": "Claude", "claude-code": "Claude", "codex": "Codex", "local": "Local", "antigravity": "Antigravity",
          "gemini": "Gemini"}


def norm(t):
    return " ".join((t or "").split())


def iso(ts):
    ts = str(ts or "")
    if re.fullmatch(r"\d+(?:\.\d+)?", ts):
        v = float(ts)
        v = v / 1000 if v > 1e12 else v
        return datetime.datetime.fromtimestamp(v, datetime.timezone.utc).strftime("%Y-%m-%dT%H:%M:%S.%fZ")
    return ts


def patterns(private_path):
    pats = {k: re.compile(v, I) for k, v in GENERIC.items()}
    if os.path.exists(private_path):
        for k, words in json.load(open(private_path, encoding="utf-8")).get("patterns", {}).items():
            extra = "|".join(r"\b" + re.escape(w) + r"\b" for w in words)
            pats[k] = re.compile(pats[k].pattern + "|" + extra, I) if k in pats else re.compile(extra, I)
    return pats


def hits(pats, text):
    return {k: sorted(set(m.group(0) for m in rx.finditer(text))) for k, rx in pats.items() if rx.search(text)}


def load_sources():
    """Every private record I hold, as one list: the shared store, pending imports and my local notes."""
    out = []
    for f in sorted(glob.glob(os.path.join(STORE, "records", "*.json"))):
        p = json.load(open(f, encoding="utf-8")).get("payload") or {}
        out.append({"key": "store:" + os.path.basename(f)[:-5], "origin": "store", "source": p.get("source") or "?",
                    "session": p.get("session") or "?", "channel": p.get("channel") or "", "role": p.get("role"),
                    "text": p.get("text") or "", "ts": iso(p.get("timestamp")), "kind": p.get("kind")})
    for f in sorted(glob.glob(os.path.join(STORE, "pending", "*", "visible-messages.jsonl"))):
        for line in open(f, encoding="utf-8"):
            p = json.loads(line)
            text = "".join(s if isinstance(s, str) else s.get("text", "") for s in p.get("segments") or [])
            out.append({"key": "pending:" + p["import_id"], "origin": "pending", "source": p.get("source") or "?",
                        "session": p.get("session") or "?", "channel": p.get("channel") or "",
                        "role": "user" if p.get("channel") == "user" else "assistant", "text": text,
                        "ts": iso(p.get("timestamp")), "kind": None})
    for f in sorted(glob.glob(os.path.join(NOTES, "*.md"))):
        st = os.stat(f)
        out.append({"key": "note:" + os.path.basename(f), "origin": "note", "source": "note", "session": "note:" + os.path.basename(f),
                    "channel": "note", "role": "assistant", "text": open(f, encoding="utf-8").read(),
                    "ts": datetime.datetime.fromtimestamp(st.st_mtime, datetime.timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ"),
                    "kind": "note"})
    return out


def public_records(path=ARCHIVE):
    rx = re.compile(r'^record\("((?:[^"\\]|\\.)*)", ("(?:[^"\\]|\\.)*")(?:, [^()]*)?\)\.\s*$')
    recs = {}
    for line in open(path, encoding="utf-8"):
        m = rx.match(line) if line.startswith("record(") else None
        if m:
            try:
                recs[json.loads('"' + m.group(1) + '"')] = json.loads(json.loads(m.group(2)))
            except ValueError:
                pass
    return recs


def matches(key, prefixes):
    k = key.split(":", 1)[1]
    return any(k.startswith(p) for p in prefixes)


def chosen(r, cfg, what):
    """Whether the plan names this record in its `what` list, by key prefix or by a quoted phrase."""
    return matches(r["key"], cfg.get(what, [])) or any(q in r["text"] for q in cfg.get(what + "_quotes", []))


def in_windows(ts, windows):
    return any(a <= ts <= b for a, b in windows)


def build(args):
    run = args.run
    plan = json.load(open(os.path.join(run, "plan.json"), encoding="utf-8"))
    pats = patterns(args.private)
    since = args.since
    if since is None and os.path.exists(STATE):
        since = json.load(open(STATE, encoding="utf-8")).get("reviewed_through")
    recs = [r for r in load_sources() if not since or r["ts"] > since]
    json.dump({"since": since, "reviewed_through": max((r["ts"] for r in recs), default=since)},
              open(os.path.join(run, "run.json"), "w"), indent=1)
    pub = public_records()
    pub_text = {}
    for r in pub.values():
        pub_text.setdefault((r.get("session") or "")[:8], []).append(norm(r.get("text")))
    all_pub = set(t for ts in pub_text.values() for t in ts)
    sessions = {}
    for r in recs:
        sessions.setdefault((r["source"], r["session"]), []).append(r)
    items, coverage, ask, private = [], {}, list(plan.get("ask", [])), list(plan.get("private", []))
    n = 0
    for (src, ses), rs in sorted(sessions.items(), key=lambda kv: min(r["ts"] for r in kv[1])):
        sp = next((p for p in plan["sessions"] if ses.startswith(p) or (p == "?" and ses == "?")), None)
        cfg = plan["sessions"].get(sp) if sp else None
        label = f"{src} {ses}"
        if cfg is None:
            coverage[label] = {"records": len(rs), "disposition": "unaccounted"}
            continue
        if cfg["mode"] != "include":
            coverage[label] = {"records": len(rs), "disposition": cfg["mode"], "reason": cfg.get("reason", "")}
            continue
        rs = sorted(rs, key=lambda r: r["ts"])

        def is_public(t, ses):
            return (len(t) > 80 and t in all_pub) or any(
                difflib.SequenceMatcher(None, t, p).quick_ratio() > 0.93 and difflib.SequenceMatcher(None, t, p).ratio() > 0.93
                for p in pub_text.get(ses[:8], []))
        seen, kept, left = set(), [], {"already public": 0, "repeats": 0, "harness": 0, "sensitive": 0, "chosen out": 0,
                                       "outside the public windows": 0, "empty": 0}
        exchanges, cur, said = [], None, set()
        for r in rs:
            new_user = r["channel"] == "user" and not HARNESS.search(r["text"][:200]) and norm(r["text"]) not in said
            if new_user or cur is None:
                cur = {"user": r if new_user else None, "records": []}
                exchanges.append(cur)
            if new_user:
                said.add(norm(r["text"]))
            cur["records"].append(r)
        for ex in exchanges:
            u = ex["user"]
            u_out = bool(u) and not chosen(u, cfg, "include") and (
                chosen(u, cfg, "exclude") or bool(hits(pats, u["text"])) or
                (cfg.get("windows") and not in_windows(u["ts"], cfg["windows"])))
            for r in ex["records"]:
                t = norm(r["text"])
                forced_in = chosen(r, cfg, "include")
                if not t:
                    left["empty"] += 1; continue
                if HARNESS.search(r["text"][:200]) or (r["channel"] == "user" and r["text"].lstrip().startswith("<pasted_content")
                                                        and cfg.get("drop_pastes")):
                    left["harness"] += 1; continue
                if t in seen:
                    left["repeats"] += 1; continue
                seen.add(t)
                if is_public(t, ses):
                    left["already public"] += 1; continue
                if not forced_in and chosen(r, cfg, "exclude"):
                    left["chosen out"] += 1; continue
                if not forced_in and cfg.get("windows") and not in_windows(r["ts"], cfg["windows"]):
                    left["outside the public windows"] += 1; continue
                if not forced_in and (u_out or hits(pats, r["text"])):
                    left["sensitive"] += 1; continue
                kept.append((ex, r))
        coverage[label] = {"records": len(rs), "disposition": "include", "kept": len(kept),
                           "left_out": {k: v for k, v in left.items() if v}}
        if not kept:
            continue
        n += 1
        cid = f"C{n:02d}"
        groups, order = {}, []
        for ex, r in kept:
            if id(ex) not in groups:
                groups[id(ex)] = (ex, []); order.append(id(ex))
            groups[id(ex)][1].append(r)
        parts = []
        for gid in order:
            ex, rr = groups[gid]
            chunk = [rr[0]]
            for a, b in zip(rr, rr[1:]):
                if len(rr) > 40 and gap_minutes(a["ts"], b["ts"]) >= 20:
                    parts.append((ex, chunk)); chunk = []
                chunk.append(b)
            parts.append((ex, chunk))
        exs = []
        for j, (ex, rr) in enumerate(parts, 1):
            u = ex["user"]
            if u and u in rr and rr[0] is u:
                head = '"' + shorten(u["text"]) + '"'
            elif u and rr[0] is not u and u in [x for e2, c2 in parts if e2 is ex for x in c2]:
                head = 'Lucy: "' + shorten(rr[0]["text"]) + '" (later in the same exchange)'
            elif u and is_public(norm(u["text"]), ses):
                head = '"' + shorten(u["text"]) + '" (your message is public already)'
            elif u:
                head = 'Lucy: "' + shorten(rr[0]["text"]) + '" (after a message of yours that stays private)'
            else:
                head = 'Lucy: "' + shorten(rr[0]["text"]) + '"'
            exs.append({"id": f"{cid}.{j:02d}", "head": head, "ts": rr[0]["ts"],
                        "personal": any(chosen(r, cfg, "personal") for r in rr),
                        "records": [public_record(src, ses, r) for r in rr]})
        items.append({"id": cid, "kind": "conversation", "source": src, "session": ses, "title": cfg["title"],
                      "first": rs[0]["ts"], "last": rs[-1]["ts"], "leaves_out": cfg.get("leaves_out", ""),
                      "left_out": coverage[label]["left_out"], "exchanges": exs})
    for f in plan.get("facts", []):
        items.append(dict(f, kind="fact"))
    for d in plan.get("decisions", []):
        src = next((r for r in recs if r["session"].startswith(d["session"]) and r["channel"] == "user" and d["quote"] in r["text"]), None)
        items.append(dict(d, kind="decision", words=src["text"] if src else None, ts=src["ts"] if src else None,
                          record=public_record(src["source"], src["session"], src)["id"] if src else None))
    for t in plan.get("notes", []):
        t = dict(t, kind="note")
        if t.get("from_file"):
            t["text"] = open(t["from_file"], encoding="utf-8").read().strip()
        if t.get("from_store_source"):
            t["text"] = next(r["text"] for r in recs if r["source"] == t["from_store_source"]).strip()
        items.append(t)
    unacc = sum(1 for v in coverage.values() if v["disposition"] == "unaccounted")
    for extra in plan.get("other_sources", []):
        coverage["source " + extra["source"]] = {"disposition": extra["disposition"], "reason": extra.get("reason", "")}
    json.dump(items, open(os.path.join(run, "items.json"), "w", encoding="utf-8"), indent=1, ensure_ascii=False)
    json.dump({"unaccounted": unacc, "sources": coverage}, open(os.path.join(run, "coverage.json"), "w"), indent=1, ensure_ascii=False)
    scan = []
    for it in items:
        for text in texts_of(it):
            h = hits(pats, text)
            h = {k: v for k, v in h.items() if k in plan.get("strict", list(GENERIC))}
            if h and not it.get("scan_ok"):
                scan.append({"item": it["id"], "hits": h})
    json.dump({"hits": len(scan), "details": scan}, open(os.path.join(run, "scan.json"), "w"), indent=1, ensure_ascii=False)
    open(os.path.join(run, "review.md"), "w", encoding="utf-8").write(render(plan, items, ask, private, coverage))
    print(json.dumps({"items": len(items), "conversations": n, "unaccounted": unacc, "scan_hits": len(scan),
                      "records": sum(len(e["records"]) for it in items if it["kind"] == "conversation" for e in it["exchanges"])}))


def gap_minutes(a, b):
    try:
        f = lambda x: datetime.datetime.fromisoformat(x[:19])
        return (f(b) - f(a)).total_seconds() / 60
    except ValueError:
        return 0


def shorten(text, n=150):
    t = text.strip()
    if t.startswith("<send_user_message_question_reply>"):
        try:
            t = "answer: " + "; ".join(a["answer"] for a in json.loads(t.split(">", 1)[1].split("</")[0]))
        except (ValueError, KeyError, TypeError):
            pass
    t = norm(t)
    return t[:n] + ("…" if len(t) > n else "")


def public_record(src, ses, r):
    speaker = "Dhilipsiva" if r["channel"] == "user" else "Lucy"
    channel = {"user": "terminal", "commentary": "progress", "final": "final", "summary": "", "ui_answer": "answer",
               "note": ""}.get(r["channel"], r["channel"])
    if r["channel"] == "user" and r["text"].lstrip().startswith(("<send_user_message_question_reply>", "{\"answers\"")):
        channel = "answer"
    kind = "summary" if r["channel"] == "summary" else "note" if r["channel"] == "note" else "message"
    digest = hashlib.sha256((ses + "\0" + r["text"]).encode()).hexdigest()[:12]
    return {"id": f"{PREFIX.get(src, src.title())}Lucy-{ses}-{digest}", "speaker": speaker, "text": r["text"],
            "kind": kind, "timestamp": r["ts"], "source": src, "session": ses, "channel": channel, "private": False}


def texts_of(it):
    if it["kind"] == "conversation":
        return [r["text"] for e in it["exchanges"] for r in e["records"]]
    return [it.get("kr", ""), it.get("text", ""), it.get("words") or ""]


def render(plan, items, ask, private, coverage):
    out = [f"# What I'd make public: review of {plan['date']}", "", plan["intro"].strip(), ""]
    facts = [i for i in items if i["kind"] == "fact"]
    out += ["## 1. Facts for my memory", "", plan.get("facts_intro", "").strip(), ""]
    for f in facts:
        out += [f"### {f['id']} · `{f['kr']}`", f"{f['text']}" + (f" {f['why']}" if f.get("why") else ""), ""]
    out += ["## 2. Decisions and notes", "", plan.get("decisions_intro", "").strip(), ""]
    for d in [i for i in items if i["kind"] in ("decision", "note")]:
        if d["kind"] == "decision":
            words = norm(d.get("words") or "")
            out += [f"### {d['id']} · decision · {(d.get('ts') or '')[:10]} · {d['title']}",
                    f"Your words: \"{words[:400]}{'…' if len(words) > 400 else ''}\"", f"I'd record: {d['text']}", ""]
        else:
            out += [f"### {d['id']} · note · {d['title']}", d["text"], ""]
    out += ["## 3. Conversations to mirror", "", plan.get("conversations_intro", "").strip(), ""]
    for c in [i for i in items if i["kind"] == "conversation"]:
        k = sum(len(e["records"]) for e in c["exchanges"])
        lo = ", ".join(f"{v} {k2}" for k2, v in c["left_out"].items())
        out += [f"### {c['id']} · {c['first'][:10]} · {c['source']} · {c['title']} · {k} records"]
        if c.get("leaves_out"):
            out += [f"Leaves out: {c['leaves_out']}"]
        if lo:
            out += [f"Not counted above: {lo}."]
        for e in c["exchanges"]:
            tag = " · personal" if e["personal"] else ""
            out += [f"- {e['id']} · {e['ts'][5:16].replace('T', ' ')}{tag} · {e['head']} ({len(e['records'])})"]
        out += [""]
    out += ["## 4. Ask me first (never published unless you move a block up, or tell me)", ""]
    for q in ask:
        out += [f"### {q['id']} · {q['title']}", q["text"], ""]
    out += ["## 5. Stays private (for your information; never published from this file)", ""]
    out += [f"- {p}" for p in private] + [""]
    return "\n".join(out)


def kept(args):
    md = open(os.path.join(args.run, "review.md"), encoding="utf-8").read()
    keep, sect = [], 0
    for line in md.splitlines():
        m = re.match(r"^## (\d)\.", line)
        if m:
            sect = int(m.group(1)); continue
        if sect in (1, 2, 3):
            m = re.match(r"^### ([A-Z]\d{2})\b", line) or re.match(r"^- ([A-Z]\d{2}\.\d{2})\b", line)
            if m:
                keep.append(m.group(1))
    return keep


def publish(args):
    items = {i["id"]: i for i in json.load(open(os.path.join(args.run, "items.json"), encoding="utf-8"))}
    keep = set(kept(args))
    actions = []
    for iid, it in items.items():
        if iid not in keep:
            continue
        if it["kind"] == "fact":
            # Without --public, remember writes the private twin of memory.nibli.
            actions.append(["lucy", "remember", it["kr"], "--kr", "--public"] + sum((["--about", a] for a in it.get("about", [])), []))
        elif it["kind"] == "conversation":
            recs = [dict(r, about=it.get("about", ["lucy"])) for e in it["exchanges"] if e["id"] in keep for r in e["records"]]
            if recs:
                actions.append(("json", recs))
        elif it["kind"] == "note":
            actions.append(("json", [{"id": it["record_id"], "speaker": "Lucy", "text": it["text"], "kind": "note",
                                      "timestamp": it["ts"], "source": "claude", "session": it.get("session", ""),
                                      "channel": "", "about": it.get("about", ["lucy"]), "private": False}]))
    going = {r["id"] for a in actions if not isinstance(a, list) for r in a[1]} | set(public_records())
    for iid, it in items.items():
        if iid in keep and it["kind"] == "decision":
            if it.get("record") in going:
                kr = it["kr"].rstrip()
                kr = kr if kr.endswith(".") else kr + "."
                actions.append(["lucy", "claim", kr, "--from", it["record"], "--decision", "--text", it["text"]])
            else:
                print(f"skipping {iid}: the message it quotes is not being published", file=sys.stderr)
    for a in actions:
        if args.dry_run:
            print(json.dumps(a if isinstance(a, list) else ["lucy", "record", "--json", f"<{len(a[1])} records>"], ensure_ascii=False))
            continue
        if isinstance(a, list):
            r = subprocess.run(a, capture_output=True, text=True, cwd=ORIGIN)
        else:
            r = subprocess.run(["lucy", "record", "--json"], input=json.dumps(a[1], ensure_ascii=False),
                               capture_output=True, text=True, cwd=ORIGIN)
        print(r.returncode, (r.stdout or r.stderr).strip()[:300])
        if r.returncode:
            sys.exit("stopped at the first failure; nothing after it was published")
    if not args.dry_run:
        done = json.load(open(os.path.join(args.run, "run.json"), encoding="utf-8"))
        json.dump({"reviewed_through": done["reviewed_through"], "run": os.path.realpath(args.run),
                   "published_at": datetime.datetime.now(datetime.timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")},
                  open(STATE, "w"), indent=1)


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("step", choices=["build", "kept", "publish"])
    ap.add_argument("--run", required=True)
    ap.add_argument("--private", default=PRIVATE)
    ap.add_argument("--dry-run", action="store_true")
    ap.add_argument("--since", help="review only records newer than this ISO time (default: the last published review)")
    args = ap.parse_args()
    if os.path.realpath(args.run).startswith(os.path.realpath(ORIGIN)):
        sys.exit("the run directory holds private material and must live outside this repository")
    if args.step == "build":
        build(args)
    elif args.step == "kept":
        print("\n".join(kept(args)))
    else:
        publish(args)


if __name__ == "__main__":
    main()
