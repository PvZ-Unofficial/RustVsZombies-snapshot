"""Summarize independent one-trial ImpLeak records by the child's release wave."""
import argparse
import collections
import json
from pathlib import Path


def summarize(records):
    failures = collections.Counter()
    groups = {}
    valid = sum(r.get("valid", 0) for r in records)
    seen = set()
    for r in records:
        if r["seed"] in seen:
            raise ValueError(f"duplicate seed: {r['seed']}")
        seen.add(r["seed"])
        if "error" in r:
            continue
        if r["valid"] + r["invalid"] != 1:
            raise ValueError("Expected independent one-trial records, not representative multi-trial samples")
        failures.update(r["failures"])
        samples = r["samples"]
        if len(samples) != r["failures"]["imp_leak"] or (samples and r["valid"] != 1):
            raise ValueError(f"seed {r['seed']}: missing or ambiguous first-failure sample")
        for s in samples:
            release = s["thrown_at"]
            if release["wave"] is None or release["time"] is None:
                raise ValueError(f"seed {r['seed']}: release wave/time unavailable")
            eligible = s.get("first_eligible_at")
            eligible_key = (eligible["wave"], eligible["time"]) if eligible else (None, None)
            key = (release["wave"], s["parent_kind"], s["parent_from_wave"], s["row"],
                   eligible_key, tuple(s["throw_context"]))
            g = groups.setdefault(key, dict(
                release_wave=release["wave"], parent_kind=s["parent_kind"],
                parent_from_wave=s["parent_from_wave"], row=s["row"],
                eligibility_wave=eligible_key[0], eligibility_time=eligible_key[1],
                throw_context=s["throw_context"], seeds=[], release_times=[],
                throwing_times=[], failure_times=[], representative=None))
            g["seeds"].append(r["seed"])
            g["release_times"].append(release["time"])
            g["throwing_times"].append(s.get("first_throwing_at"))
            g["failure_times"].append(s["failed_at"])
            if g["representative"] is None or r["seed"] < g["representative"]["seed"]:
                g["representative"] = dict(seed=r["seed"], first_throwing_at=s.get("first_throwing_at"),
                                           thrown_at=release, failed_at=s["failed_at"])
    for g in groups.values():
        g["first_failure_trials"] = len(g["seeds"])
        g["first_failure_rate_over_valid_trials"] = len(g["seeds"]) / valid if valid else None
        g["release_time_min"] = min(g["release_times"])
        g["release_time_max"] = max(g["release_times"])
    ordered = sorted(groups.values(), key=lambda g: (
        g["release_wave"], g["release_time_min"], g["parent_from_wave"], g["row"], g["parent_kind"]))
    return dict(
        grouping="release_wave_parent_origin_row_eligibility_context",
        interval_basis="observed_release_times_of_first_failure_imps",
        probability_basis="first_failure_trials_divided_by_valid_trials",
        count=len(records), valid=valid, invalid=sum(r.get("invalid", 0) for r in records),
        errors=[r for r in records if "error" in r], failures=dict(failures),
        success=sum(r.get("success", 0) for r in records), groups=ordered)


def point(p):
    if p is None:
        return "未观测"
    if p["wave"] is None or p["time"] is None:
        return f"绝对 {p['main_counter']}cs"
    return f"W{p['wave']}:{p['time']}"


def markdown(report):
    lines = [
        "# 漏小鬼：按离手时间归组", "",
        f"试验 {report['count']} 轮，有效 {report['valid']}，invalid {report['invalid']}，"
        f"执行错误 {len(report['errors'])}，漏鬼首败 {report['failures'].get('imp_leak', 0)}。",
        "",
        "区间是该类首败小鬼实际离手时刻的观测范围；概率分母为全部有效试验，"
        "不是落入该离手区间的小鬼数量。其他原因先败会终止本轮后续观察。"
        "同一离手区间内可能也有安全小鬼，不能把区间解释成必漏窗口。",
        "",
        "| 小鬼离手波次与区间（cs） | 首败次数／频率 | 父巨人来源／行 | 代表 seed | 开始投掷 | 实际离手 | 判漏 |",
        "|---|---:|---|---:|---|---|---|",
    ]
    for g in report["groups"]:
        e = g["representative"]
        lines.append(
            f"| W{g['release_wave']} [{g['release_time_min']}, {g['release_time_max']}] "
            f"| {g['first_failure_trials']}／{g['first_failure_rate_over_valid_trials']:.2%} "
            f"| W{g['parent_from_wave']}／R{g['row']} ({g['parent_kind']}) | {e['seed']} "
            f"| {point(e['first_throwing_at'])} | {point(e['thrown_at'])} | {point(e['failed_at'])} |")
    return "\n".join(lines) + "\n"


def self_test():
    def row(seed, wave, release, failed_wave):
        s = dict(parent_kind="giga_gargantuar", parent_from_wave=13, row=2, throw_context=[],
                 first_eligible_at=None, first_throwing_at=None,
                 thrown_at=dict(main_counter=release, wave=wave, time=release),
                 failed_at=dict(main_counter=1000, wave=failed_wave, time=200))
        return dict(seed=seed, valid=1, invalid=0, failures=dict(imp_leak=1), success=0, samples=[s])
    records = [row(1, 14, 467, 15), row(2, 14, 473, 16), row(3, 15, 3, 15),
               dict(seed=4, valid=1, invalid=0, failures=dict(imp_leak=0), success=1, samples=[]),
               dict(seed=5, valid=0, invalid=1, failures=dict(imp_leak=0), success=0, samples=[])]
    report = summarize(records)
    a, b = report["groups"]
    assert (a["release_wave"], a["release_time_min"], a["release_time_max"]) == (14, 467, 473)
    assert a["first_failure_rate_over_valid_trials"] == 0.5
    assert [p["wave"] for p in a["failure_times"]] == [15, 16]
    assert b["release_wave"] == 15 and report["valid"] == 4 and report["invalid"] == 1
    assert "W14 [467, 473]" in markdown(report)
    try:
        summarize([records[0], records[0]])
    except ValueError:
        pass
    else:
        raise AssertionError("duplicate trials must not change the denominator")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("samples", type=Path, nargs="?")
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()
    if args.self_test:
        self_test()
        print("release grouping checks passed")
        return
    if args.samples is None:
        parser.error("samples.json is required")
    records = json.loads(args.samples.read_text(encoding="utf-8-sig"))
    report = summarize(records)
    folder = args.samples.parent
    previous = folder / "summary.json"
    if previous.exists():
        old = json.loads(previous.read_text(encoding="utf-8-sig"))
        if "elapsed_seconds" in old:
            report["elapsed_seconds"] = old["elapsed_seconds"]
    previous.write_text(json.dumps(report, ensure_ascii=False, indent=2), encoding="utf-8")
    (folder / "summary.md").write_text(markdown(report), encoding="utf-8")
    print(f"release summary: {folder / 'summary.md'}")


if __name__ == "__main__":
    main()
