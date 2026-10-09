import { useCallback, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { isPending, newestFirst, problemVerdict, submissionProblem } from "./judging";
import {
  problemKey,
  type Paper,
  type Problem,
  type Records,
  type Submission,
  type Site,
} from "./types";
import { ojProgressCache, progressScope, progressScopeKey } from "./progress-store";

export function useSubmissions(
  siteId: string,
  contestId: string,
  paper?: Paper,
  problem?: Problem,
  site?: Site,
) {
  const key = problem ? problemKey(problem) : "";
  const scope = `${siteId}/${contestId}/${paper?.account}/${paper?.epoch}`;
  const current = `${scope}/${key}`;
  const currentRef = useRef(current);
  currentRef.current = current;
  const [history, setHistory] = useState<{ current: string; value: Records }>();
  const [byKind, setByKind] = useState<Record<number, Submission[]>>({});
  const [directoryState, setDirectoryState] = useState<Record<number, "ready" | "failed">>({});
  const [recordError, setRecordError] = useState<string>();
  const [view, setView] = useState<{ current: string; submission: Submission }>();
  const [details, setDetails] = useState<{ current: string; rows: Record<string, Submission> }>();
  const [watch, setWatch] = useState<{ current: string; submission: Submission; nonce: number }>();
  const [polling, setPolling] = useState(false);
  const [pollError, setPollError] = useState<string>();
  const refreshGeneration = useRef(0);
  const observing = useRef(false);
  const cacheScope = site ? progressScope(site) : undefined;
  const cacheKey = cacheScope ? progressScopeKey(cacheScope) : "";
  useEffect(() => {
    if (
      !paper ||
      !cacheScope ||
      (site?.sessionGeneration != null && site.sessionGeneration !== paper.epoch)
    )
      return;
    const kinds = [
      ...new Set(paper.problems.filter((p) => [0, 4].includes(p.kind)).map((p) => p.kind)),
    ];
    if (!kinds.every((kind) => directoryState[kind] === "ready")) return;
    const accepted = paper.problems.filter(
      (p) =>
        problemVerdict((byKind[p.kind] ?? []).filter((r) => submissionProblem(r) === p.id)) === 0,
    ).length;
    ojProgressCache.seed(cacheScope, contestId, { accepted, total: paper.problems.length });
  }, [cacheKey, contestId, paper, byKind, directoryState]);

  const upsert = useCallback((submission: Submission, kind: number, pk: string) => {
    const value = { ...submission, problem: pk.split(":")[1] };
    setByKind((old) => {
      const rows = old[kind] ?? [];
      const previous = rows.find((r) => String(r.id) === String(value.id));
      return {
        ...old,
        [kind]: [
          { ...previous, ...value },
          ...rows.filter((r) => String(r.id) !== String(value.id)),
        ],
      };
    });
    setHistory((old) =>
      old?.current === currentRef.current && currentRef.current.endsWith(`/${pk}`)
        ? {
            ...old,
            value: {
              ...old.value,
              records: [
                value,
                ...old.value.records.filter((r) => String(r.id) !== String(value.id)),
              ].sort(newestFirst),
            },
          }
        : old,
    );
  }, []);
  const show = useCallback(
    (submission: Submission) => {
      if (!problem || currentRef.current !== current) return;
      observing.current = true;
      setView({ current, submission });
      setPollError(undefined);
      setWatch({ current, submission, nonce: Date.now() });
      upsert(submission, problem.kind, key);
    },
    [current, problem?.kind, key, upsert],
  );

  const refreshRecords = useCallback(async () => {
    if (!paper || !problem) return;
    const generation = ++refreshGeneration.current;
    setRecordError(undefined);
    try {
      const value = await invoke<Records>("get_oj_records", {
        siteId,
        contestId,
        kind: problem.kind,
        problemId: problem.id,
      });
      if (currentRef.current !== current || generation !== refreshGeneration.current) return;
      // Defense in depth, also excluding rows with no problem identity.
      const records = value.records
        .filter((r) => submissionProblem(r) === problem.id)
        .sort(newestFirst);
      const uncertain = value.uncertain.filter(
        (u) =>
          u.details.problemKey === key || (!u.details.problemKey && ![0, 4].includes(problem.kind)),
      );
      setHistory({ current, value: { records, uncertain } });
      for (const row of records) upsert(row, problem.kind, key);
      if (
        [0, 4].includes(problem.kind) &&
        records[0] &&
        isPending(records[0]) &&
        !observing.current
      )
        show(records[0]);
    } catch (e) {
      if (currentRef.current === current && generation === refreshGeneration.current)
        setRecordError(String(e));
    }
  }, [siteId, contestId, current, paper, problem?.id, problem?.kind, key, upsert, show]);

  useEffect(() => {
    let active = true;
    setByKind({});
    setDirectoryState({});
    if (paper)
      for (const kind of [0, 4].filter((kind) => paper.problems.some((p) => p.kind === kind))) {
        void invoke<Records>("get_oj_records", { siteId, contestId, kind })
          .then((value) => {
            if (active) setDirectoryState((old) => ({ ...old, [kind]: "ready" }));
            if (active)
              setByKind((old) => {
                const fresh = old[kind] ?? [];
                return {
                  ...old,
                  [kind]: [
                    ...fresh,
                    ...value.records.filter(
                      (r) => !fresh.some((n) => String(n.id) === String(r.id)),
                    ),
                  ],
                };
              });
          })
          .catch((e) => {
            if (active) {
              setDirectoryState((old) => ({ ...old, [kind]: "failed" }));
              setRecordError(String(e));
            }
          });
      }
    return () => {
      active = false;
    };
  }, [siteId, contestId, scope, paper]);
  useEffect(() => {
    observing.current = false;
    setWatch(undefined);
    setView(undefined);
    setHistory(undefined);
    setPollError(undefined);
    void refreshRecords();
    return () => {
      refreshGeneration.current++;
    };
  }, [refreshRecords]);

  useEffect(() => {
    if (!watch || watch.current !== current || !problem) {
      setPolling(false);
      return;
    }
    let active = true;
    let timer: ReturnType<typeof setTimeout>;
    let attempts = 0;
    let failures = 0;
    let latest = watch.submission;
    setPolling(true);
    const run = async () => {
      if (!active) return;
      attempts++;
      try {
        const next = await invoke<Submission>("get_oj_result", {
          siteId,
          submissionId: String(watch.submission.id),
          kind: problem.kind,
          problemId: problem.id,
        });
        if (!active) return;
        latest = { ...latest, ...next };
        setView({ current, submission: latest });
        setDetails((old) => ({
          current,
          rows: { ...(old?.current === current ? old.rows : {}), [String(latest.id)]: latest },
        }));
        upsert(latest, problem.kind, key);
        failures = 0;
        setPollError(undefined);
        if (![0, 4].includes(problem.kind) || !isPending(latest)) {
          setPolling(false);
          observing.current = false;
          return;
        }
      } catch (e) {
        if (!active) return;
        failures++;
        setPollError(
          `获取判题结果失败：${String(e)}${failures < 3 ? "，稍后自动重试。" : "，请手动刷新判题；不会重新提交代码。"}`,
        );
      }
      if (failures >= 3 || attempts >= 90) {
        if (attempts >= 90) setPollError("判题仍未结束，请稍后刷新判题；不会重新提交代码。");
        observing.current = false;
        setPolling(false);
        return;
      }
      timer = setTimeout(() => void run(), failures ? 2000 * 2 ** failures : 2000);
    };
    void run();
    return () => {
      active = false;
      clearTimeout(timer);
    };
  }, [watch, current, siteId, problem?.id, problem?.kind, key, upsert]);

  useEffect(() => {
    let active = true;
    let stop: (() => void) | undefined;
    void listen<{
      siteId: string;
      contestId: string;
      problemKey: string;
      account: string;
      epoch: number;
      submission: Submission;
    }>("oj-companion-result", ({ payload }) => {
      if (
        !active ||
        payload.siteId !== siteId ||
        payload.contestId !== contestId ||
        payload.account !== paper?.account ||
        payload.epoch !== paper?.epoch
      )
        return;
      upsert(payload.submission, Number(payload.problemKey.split(":")[0]), payload.problemKey);
      if (payload.problemKey === key) show(payload.submission);
    })
      .then((off) => {
        if (active) stop = off;
        else off();
      })
      .catch(() => {});
    return () => {
      active = false;
      stop?.();
    };
  }, [siteId, contestId, scope, key, show, upsert]);

  return {
    records: history?.current === current ? history.value : undefined,
    result: view?.current === current ? view.submission : undefined,
    detail: (s: Submission) =>
      details?.current === current ? (details.rows[String(s.id)] ?? s) : s,
    polling: watch?.current === current && polling,
    pollError,
    recordError,
    refreshRecords,
    show,
    verdict: (p: Problem) =>
      problemVerdict((byKind[p.kind] ?? []).filter((r) => submissionProblem(r) === p.id)),
    emptyVerdict: (p: Problem) => {
      if ([0, 4].includes(p.kind))
        return directoryState[p.kind] === "ready"
          ? "未提交"
          : directoryState[p.kind] === "failed"
            ? "读取失败"
            : "查询中";
      const answer = paper?.drafts[problemKey(p)]?.answer;
      return Array.isArray(answer)
        ? answer.length
          ? "已作答"
          : "未作答"
        : answer
          ? "已作答"
          : "未作答";
    },
  };
}
