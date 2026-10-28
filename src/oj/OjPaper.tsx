import { lazy, Suspense, useCallback, useEffect, useMemo, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import {
  Checkbox,
  Dialog,
  Drawer,
  LoadingIndicator,
  Radio,
  Radios,
  Select,
  SelectOption,
  Tab,
  Tabs,
  TextField,
  Button,
} from "material/react";
import { useSelector } from "@tanstack/react-store";
import { ojProgressCache, progressScope, progressScopeKey } from "./progress-store";
import { Banner, Chip } from "../ui";
import { nameTablist } from "../ui/tablist";
import renderMathInElement from "katex/contrib/auto-render";
import "katex/dist/katex.min.css";
import { useLogicalNavigation } from "../logical-navigation";
import { copyText } from "../clipboard";
import { cleanHtml, hasContestDescription, problemMarkdown } from "./content";
import { CodeEditor } from "./CodeEditor";
import { CompileOptions } from "./CompileOptions";
import { compileOptions } from "./compile-options";
import { notifySuccess } from "./feedback";
import { SampleField } from "./SampleField";
import { ProblemList } from "./ProblemList";
import { problemStatus, statusPillHtml } from "./problem-status";
import { useOjLayout } from "./layout";
import { SubmissionRecord, ResultAccordion } from "./SubmissionRecord";
import { RankingTable } from "./RankingTable";
import { ContestStatus, formatRemaining } from "./ContestTable";
import { useSubmissions } from "./useSubmissions";
const MonacoEditor = lazy(() => import("./MonacoEditor"));
import {
  type Site,
  type Paper,
  type Preferences,
  type Draft,
  type Problem,
  type Submission,
  problemKey,
  kindName,
  languageName,
  route,
} from "./types";
export function Html({ html, base, siteId }: { html: string; base: string; siteId?: string }) {
  const element = useRef<HTMLDivElement>(null);
  const safeHtml = useMemo(() => {
    const el = document.createElement("div");
    el.innerHTML = cleanHtml(html, base);
    if (siteId)
      for (const img of el.querySelectorAll("img[src]")) {
        const src = img.getAttribute("src")!;
        if (new URL(src).origin === new URL(base).origin) {
          img.setAttribute("data-oj-image", src);
          img.removeAttribute("src");
        }
      }
    return el.innerHTML;
  }, [html, base, siteId]);
  useEffect(() => {
    let live = true;
    if (siteId && element.current)
      for (const img of element.current.querySelectorAll<HTMLImageElement>("img[data-oj-image]")) {
        void invoke<string>("load_oj_image", { siteId, url: img.dataset.ojImage })
          .then((src) => {
            if (live) img.src = src;
          })
          .catch(() => {
            if (live) img.alt += "（图片暂不可访问）";
          });
      }
    return () => {
      live = false;
    };
  }, [safeHtml, siteId]);
  useEffect(() => {
    if (element.current)
      renderMathInElement(element.current, {
        throwOnError: false,
        trust: false,
        delimiters: [
          { left: "$$", right: "$$", display: true },
          { left: "\\[", right: "\\]", display: true },
          { left: "\\(", right: "\\)", display: false },
          { left: "$", right: "$", display: false },
        ],
      });
  }, [html, base]);
  return (
    <div className="oj-statement" ref={element} dangerouslySetInnerHTML={{ __html: safeHtml }} />
  );
}
function Statement({ p, paper, siteId }: { p: Problem; paper: Paper; siteId: string }) {
  return (
    <div className="oj-statement-sections">
      <div className="oj-actions">
        <Chip>{kindName[p.kind]}</Chip>
        {p.score != null && <Chip>{p.score} 分</Chip>}
        {p.timeLimit > 0 && (
          <span>
            {p.timeLimit} s · {p.memoryLimit} MB
          </span>
        )}
      </div>
      {[
        ["题目要求", p.description],
        ["输入说明", p.input],
        ["输出说明", p.output],
      ].map(
        ([title, html]) =>
          html && (
            <section key={title}>
              <h3>{title}</h3>
              <Html html={html} base={paper.sourceUrl} siteId={siteId} />
            </section>
          ),
      )}
      {!!p.choices.length && (
        <section>
          <h3>选项</h3>
          {p.choices.map((value, i) => (
            <div key={i} className="oj-choice">
              <strong>{String.fromCharCode(65 + i)}.</strong>
              <Html html={value} base={paper.sourceUrl} siteId={siteId} />
            </div>
          ))}
        </section>
      )}
      {p.samples.map((s, i) => (
        <section key={i}>
          <h3>样例 {i + 1}</h3>
          <div className="oj-sample">
            <SampleField label="输入" value={s.input} sample={i + 1} />
            <SampleField label="输出" value={s.output} sample={i + 1} />
          </div>
        </section>
      ))}
      {p.hint && (
        <section>
          <h3>提示</h3>
          <Html html={p.hint} base={paper.sourceUrl} siteId={siteId} />
        </section>
      )}
      {Object.entries(p.templates).map(([language, value]) => (
        <section key={language}>
          <h3>代码模板 · {languageName[language] ?? language}</h3>
          <pre>{value}</pre>
        </section>
      ))}
    </div>
  );
}
export function OjPaper({
  site,
  contestId,
  groupId,
  selected,
  tab,
  desktop,
  port,
}: {
  site: Site;
  contestId: string;
  groupId?: string;
  selected?: string;
  tab: string;
  desktop: boolean;
  port: number;
}) {
  const nav = useLogicalNavigation();
  const [paper, setPaper] = useState<Paper>();
  const showContestDescription = useMemo(
    () => (paper ? hasContestDescription(paper.contest.description, paper.sourceUrl) : false),
    [paper?.contest.description, paper?.sourceUrl],
  );
  const [drafts, setDrafts] = useState<Record<string, Draft>>({});
  const [incomingDrafts, setIncomingDrafts] = useState<Record<string, Draft>>({});
  const draftsRef = useRef(drafts);
  draftsRef.current = drafts;
  const preferencesRef = useRef<Preferences>({
    language: "cpp",
    optionsByLanguage: {},
    revision: 0,
  });
  const preferenceSaving = useRef(false);
  const pendingPreferences = useRef<Preferences>(undefined);
  const [error, setError] = useState<string>();
  const [busy, setBusy] = useState(false);
  const [saved, setSaved] = useState("已保存");
  const [confirm, setConfirm] = useState(false);
  const [manualCopy, setManualCopy] = useState<string>();
  const [sheetOpen, setSheetOpen] = useState(false);
  const { layout, reading, paperRef, detailRef } = useOjLayout();
  const [now, setNow] = useState(Date.now());
  const dirty = useRef<Record<string, Draft>>({});
  const loaded = useRef<Paper>(undefined);
  const timer = useRef<ReturnType<typeof setTimeout>>(undefined);
  const writes = useRef<Promise<void>>(Promise.resolve());
  const live = useRef(true);
  const timeOffset = useRef(0);
  const flush = useCallback(() => {
    clearTimeout(timer.current);
    const current = loaded.current;
    if (!current) return writes.current;
    writes.current = writes.current
      .catch(() => {})
      .then(async () => {
        const pending = dirty.current;
        dirty.current = {};
        const entries = Object.entries(pending);
        for (let index = 0; index < entries.length; index++) {
          const [key, draft] = entries[index];
          try {
            await invoke("save_oj_draft", {
              input: {
                siteId: site.id,
                contestId,
                problemKey: key,
                account: current.account,
                epoch: current.epoch,
                draft,
              },
            });
          } catch (e) {
            for (const [remainingKey, remaining] of entries.slice(index))
              if (!dirty.current[remainingKey]) dirty.current[remainingKey] = remaining;
            throw e;
          }
        }
        if (live.current && !Object.keys(dirty.current).length) setSaved("已保存到本机");
      });
    return writes.current;
  }, [site.id, contestId]);
  const load = useCallback(async () => {
    const next = await invoke<Paper>("get_oj_paper", { siteId: site.id, contestId });
    if (!live.current) return;
    loaded.current = next;
    const firstDraft = Object.values(next.drafts).find((draft) => draft.language);
    preferencesRef.current = next.preferences ?? {
      language: firstDraft?.language ?? "cpp",
      optionsByLanguage: firstDraft?.options ? { [firstDraft.language]: firstDraft.options } : {},
      revision: 0,
    };
    setPaper(next);
    setDrafts(
      Object.fromEntries(
        Object.entries(next.drafts).map(([key, draft]) => [
          key,
          { ...draft, options: compileOptions(draft.language, draft.options) },
        ]),
      ),
    );
    timeOffset.current = next.contest.now - Date.now();
    setNow(next.contest.now);
  }, [site.id, contestId]);
  useEffect(() => {
    live.current = true;
    setBusy(true);
    void load()
      .catch((e) => setError(String(e)))
      .finally(() => setBusy(false));
    const clock = setInterval(() => setNow(Date.now() + timeOffset.current), 1000);
    return () => {
      live.current = false;
      clearInterval(clock);
      void flush().catch(() => {});
    };
  }, [load, flush]);
  const applyPreferences = useCallback(
    async (preferences: Preferences) => {
      const current = loaded.current;
      if (!current || preferences.revision < preferencesRef.current.revision) return;
      const next = { ...draftsRef.current };
      for (const problem of current.problems.filter((p) => [0, 4].includes(p.kind))) {
        const key = problemKey(problem);
        const existing = next[key];
        const saved =
          existing?.language === preferences.language
            ? existing
            : await invoke<Draft | null>("get_oj_draft", {
                siteId: site.id,
                contestId,
                problemKey: key,
                language: preferences.language,
              });
        next[key] = {
          ...(saved ?? {
            code: problem.templates[preferences.language] ?? "",
            answer: "",
            revision: 0,
          }),
          language: preferences.language,
          options: compileOptions(
            preferences.language,
            preferences.optionsByLanguage[preferences.language],
          ),
        };
      }
      if (live.current && preferences.revision >= preferencesRef.current.revision) {
        preferencesRef.current = preferences;
        setDrafts(next);
      }
    },
    [site.id, contestId],
  );
  useEffect(() => {
    let active = true;
    let stop: (() => void) | undefined;
    void listen<{ siteId: string; preferences: Preferences }>(
      "oj-preferences-changed",
      ({ payload }) => {
        if (
          payload.siteId === site.id &&
          payload.preferences.revision > preferencesRef.current.revision
        ) {
          if (preferenceSaving.current) {
            pendingPreferences.current = payload.preferences;
            return;
          }
          setBusy(true);
          void flush()
            .then(() => applyPreferences(payload.preferences))
            .catch((e) => setError(String(e)))
            .finally(() => {
              if (live.current) setBusy(false);
            });
        }
      },
    )
      .then((off) => {
        if (active) stop = off;
        else off();
      })
      .catch(() => {});
    return () => {
      active = false;
      stop?.();
    };
  }, [site.id, flush, applyPreferences]);
  useEffect(() => {
    let active = true;
    let stop: (() => void) | undefined;
    void listen<{
      siteId: string;
      contestId: string;
      problemKey: string;
      account: string;
      epoch: number;
      draft: Draft;
    }>("oj-draft-changed", ({ payload }) => {
      const current = loaded.current;
      if (
        !active ||
        payload.siteId !== site.id ||
        payload.contestId !== contestId ||
        payload.account !== current?.account ||
        payload.epoch !== current?.epoch
      )
        return;
      const key = payload.problemKey;
      if (dirty.current[key]) {
        setIncomingDrafts((old) => ({ ...old, [key]: payload.draft }));
        setSaved("cph-ng 代码已缓存，编辑器另有未保存修改");
        clearTimeout(timer.current);
        return;
      }
      if (payload.draft.language === preferencesRef.current.language) {
        setDrafts((old) =>
          old[key] && old[key].revision >= payload.draft.revision
            ? old
            : { ...old, [key]: payload.draft },
        );
        setSaved("cph-ng 代码已缓存到本机");
      }
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
  }, [site.id, contestId]);
  const savePreferences = async (language: string, options: Draft["options"]) => {
    preferenceSaving.current = true;
    try {
      await flush();
      const current = loaded.current!;
      const next = await invoke<Preferences>("save_oj_preferences", {
        input: {
          siteId: site.id,
          account: current.account,
          epoch: current.epoch,
          expectedRevision: preferencesRef.current.revision,
          language,
          options,
        },
      });
      await applyPreferences(next);
      setSaved("实例编译配置已保存");
    } finally {
      preferenceSaving.current = false;
      const pending = pendingPreferences.current;
      pendingPreferences.current = undefined;
      if (pending && pending.revision > preferencesRef.current.revision)
        await applyPreferences(pending);
    }
  };
  const p = paper?.problems.find((p) => problemKey(p) === selected) ?? paper?.problems[0];
  const pk = p ? problemKey(p) : "";
  const draft = drafts[pk];
  const submissions = useSubmissions(site.id, contestId, paper, p, site);
  const { records, result, refreshRecords } = submissions;
  const completionKey = progressScopeKey(progressScope(site));
  const completion = useSelector(
    ojProgressCache.store,
    (state) => state[completionKey]?.entries[contestId]?.value,
  );
  const writable =
    !!paper?.writable &&
    now >= paper.contest.begin &&
    now < paper.contest.end &&
    site.session === "ready";
  const go = (updates: { problem?: string; tab?: string }) =>
    nav.replace(
      route({
        site: site.id,
        group: groupId,
        contest: contestId,
        problem: updates.problem ?? pk,
        tab: updates.tab ?? tab,
      }),
    );
  const edit = (next: Partial<Draft>) => {
    if (!draft || !writable) return;
    const value = { ...draft, ...next, revision: Math.max(Date.now(), draft.revision + 1) };
    setDrafts((old) => ({ ...old, [pk]: value }));
    dirty.current[pk] = value;
    if (incomingDrafts[pk]) return;
    setSaved("正在保存…");
    clearTimeout(timer.current);
    timer.current = setTimeout(() => {
      void flush().catch((e) => {
        setSaved("保存失败");
        setError(String(e));
      });
    }, 450);
  };
  const action = async (run: () => Promise<unknown>) => {
    setBusy(true);
    setError(undefined);
    try {
      await run();
    } catch (e) {
      if (live.current) setError(String(e));
    } finally {
      if (live.current) setBusy(false);
    }
  };
  const submit = (whole: boolean) =>
    action(async () => {
      await flush();
      const value = await invoke<Submission>("submit_oj", {
        input: {
          siteId: site.id,
          contestId,
          problemKey: whole ? null : pk,
          account: paper!.account,
          epoch: paper!.epoch,
        },
      });
      setConfirm(false);
      if (whole) {
        notifySuccess("答卷已提交；以服务器记录为准。");
        await load();
      } else {
        notifySuccess("代码已提交，正在查询判题结果。");
        submissions.show(value);
      }
    });
  const send = (all = false) =>
    action(async () => {
      if (!Number.isInteger(port) || port < 1 || port > 65535) throw new Error("请输入有效端口");
      const response = await invoke<{ sent: number }>("send_oj_companion", {
        siteId: site.id,
        contestId,
        keys: all ? [] : [pk],
        port,
      });
      notifySuccess(`已向本机工具发送 ${response.sent} 道题。`);
    });
  if (!paper)
    return (
      <>
        {busy && <LoadingIndicator size={24} aria-label="加载测验" />}
        {error && <Banner status="warning">{error}</Banner>}
        <Button variant="tonal" onClick={() => void action(load)}>
          重新加载
        </Button>
      </>
    );
  const recordPanel = (
    <div className="oj-form">
      <Button variant="tonal" disabled={busy} onClick={() => void action(refreshRecords)}>
        刷新提交记录
      </Button>
      {submissions.recordError && <Banner status="warning">{submissions.recordError}</Banner>}
      {!records && !submissions.recordError && (
        <LoadingIndicator size={24} aria-label="正在读取当前题目提交记录" />
      )}
      {records && records.records.length === 0 && (
        <p className="oj-muted">当前题目暂无提交记录。</p>
      )}
      {records?.uncertain.map((u) => (
        <Banner status="warning" key={u.key}>
          <Banner.Title>上次提交结果待确认</Banner.Title>
          <Banner.Description>
            {new Date(u.details.time).toLocaleString()}
            。请先核对以下记录，再确认是否需要重新提交。
          </Banner.Description>
          <Button
            variant="tonal"
            onClick={() =>
              void action(async () => {
                await invoke("acknowledge_oj_submission", { siteId: site.id, key: u.key });
                await refreshRecords();
              })
            }
          >
            已核对，解除待确认状态
          </Button>
        </Banner>
      ))}
      {records?.records.map((r) => (
        <SubmissionRecord
          key={r.id}
          submission={submissions.detail(r)}
          desktop={desktop}
          polling={String(result?.id) === String(r.id) && submissions.polling}
          error={String(result?.id) === String(r.id) ? submissions.pollError : undefined}
          onShow={() => submissions.show(submissions.detail(r))}
        />
      ))}
    </div>
  );
  const answerPanel =
    p && draft ? (
      <div className="oj-form">
        <div className="oj-toolbar">
          <strong>作答</strong>
          <span role="status">{saved}</span>
        </div>
        {incomingDrafts[pk] && (
          <Banner status="warning">
            <Banner.Title>cph-ng 代码已缓存，编辑器另有未保存修改</Banner.Title>
            <Banner.Description>当前编辑内容已保留，请选择继续使用哪一份代码。</Banner.Description>
            <div className="oj-actions">
              <Button
                variant="tonal"
                disabled={busy}
                onClick={() =>
                  void action(async () => {
                    await writes.current.catch(() => {});
                    const next = {
                      ...draft,
                      revision: Math.max(Date.now(), incomingDrafts[pk].revision + 1),
                    };
                    dirty.current[pk] = next;
                    setDrafts((old) => ({ ...old, [pk]: next }));
                    await flush();
                    setIncomingDrafts((old) => {
                      const next = { ...old };
                      delete next[pk];
                      return next;
                    });
                  })
                }
              >
                保留编辑器修改
              </Button>
              <Button
                variant="tonal"
                disabled={busy}
                onClick={() =>
                  void action(async () => {
                    await writes.current.catch(() => {});
                    delete dirty.current[pk];
                    setDrafts((old) => ({ ...old, [pk]: incomingDrafts[pk] }));
                    setIncomingDrafts((old) => {
                      const next = { ...old };
                      delete next[pk];
                      return next;
                    });
                    setSaved("cph-ng 代码已缓存到本机");
                  })
                }
              >
                载入 cph-ng 缓存
              </Button>
            </div>
          </Banner>
        )}
        {!writable && <Banner status="default">当前测验不可提交，已保存草稿仍可查看。</Banner>}
        {[0, 4].includes(p.kind) ? (
          <>
            <p className="oj-muted">语言和编译选项保存于当前 OJ 实例，切换题目时沿用。</p>
            {!p.languages.includes(draft.language) && (
              <Banner status="warning">当前题目不支持已保存的语言，请手动选择支持的语言。</Banner>
            )}
            <Select
              label="语言"
              aria-label="编程语言"
              value={draft.language || ""}
              disabled={!writable || busy}
              onChange={(e: any) => {
                const language = e.detail?.value ?? e.target?.value;
                if (!language) return;
                void action(async () => {
                  await savePreferences(
                    String(language),
                    compileOptions(
                      String(language),
                      preferencesRef.current.optionsByLanguage[String(language)],
                    ),
                  );
                });
              }}
            >
              {[...new Set([draft.language, ...p.languages])].filter(Boolean).map((language) => (
                <SelectOption
                  key={language}
                  value={language}
                  disabled={!p.languages.includes(language)}
                >
                  {languageName[language] ?? language}
                </SelectOption>
              ))}
            </Select>
            <CompileOptions
              language={draft.language}
              value={draft.options}
              disabled={!writable || busy}
              onChange={(options) => void action(() => savePreferences(draft.language, options))}
            />
            {desktop ? (
              <Suspense fallback={<LoadingIndicator size={24} aria-label="加载桌面编辑器" />}>
                <MonacoEditor
                  value={draft.code}
                  language={draft.language}
                  disabled={!writable || busy}
                  onChange={(code) => edit({ code })}
                />
              </Suspense>
            ) : (
              <CodeEditor
                value={draft.code}
                language={draft.language}
                disabled={!writable || busy}
                onChange={(code) => edit({ code })}
              />
            )}
            <Button
              disabled={
                busy ||
                submissions.polling ||
                !writable ||
                !!incomingDrafts[pk] ||
                !draft.code.trim() ||
                !p.languages.includes(draft.language)
              }
              onClick={() => void submit(false)}
            >
              提交代码
            </Button>
          </>
        ) : p.kind === 1 ? (
          p.multiple ? (
            <div className="grid gap-2" role="group" aria-label="选择答案">
              <span className="font-semibold text-xs text-muted mb-1 block">多选</span>
              {p.choices.map((c, i) => {
                const isChecked =
                  Array.isArray(draft.answer) && draft.answer.map(String).includes(String(i));
                return (
                  <Checkbox
                    key={i}
                    value={String(i)}
                    checked={isChecked}
                    disabled={!writable}
                    onChange={(e: any) => {
                      const checked = Boolean(e.detail?.checked ?? e.target?.checked);
                      const current = Array.isArray(draft.answer) ? draft.answer.map(Number) : [];
                      const next = checked
                        ? [...current.filter((x) => x !== i), i].sort()
                        : current.filter((x) => x !== i);
                      edit({ answer: next });
                    }}
                  >
                    {String.fromCharCode(65 + i)}. {plain(c)}
                  </Checkbox>
                );
              })}
            </div>
          ) : (
            <div className="grid gap-2">
              <span className="font-semibold text-xs text-muted mb-1 block">单选</span>
              <Radios
                aria-label="选择答案"
                value={Array.isArray(draft.answer) ? String(draft.answer[0] ?? "") : ""}
                disabled={!writable}
                onChange={(e: any) => {
                  const val = e.detail?.value ?? e.target?.value;
                  if (val !== undefined && val !== "") edit({ answer: [Number(val)] });
                }}
              >
                {p.choices.map((c, i) => (
                  <Radio key={i} value={String(i)}>
                    {String.fromCharCode(65 + i)}. {plain(c)}
                  </Radio>
                ))}
              </Radios>
            </div>
          )
        ) : p.kind === 2 ? (
          Array.from({ length: p.blanks }, (_, i) => (
            <TextField
              label="第 {i + 1} 空"
              key={i}
              disabled={!writable}
              value={String(Array.isArray(draft.answer) ? (draft.answer[i] ?? "") : "")}
              onChange={(e: any) => {
                const value = e.detail?.value ?? e.target?.value ?? "";
                const values = Array.from({ length: p.blanks }, (_, j) =>
                  String(Array.isArray(draft.answer) ? (draft.answer[j] ?? "") : ""),
                );
                values[i] = value;
                edit({ answer: values });
              }}
            />
          ))
        ) : (
          <div className="space-y-1.5">
            <label className="font-semibold text-xs text-muted block">问答题作答</label>
            <TextField
              type="multiline"
              label="问答题作答"
              disabled={!writable}
              value={typeof draft.answer === "string" ? draft.answer : ""}
              onChange={(event) => edit({ answer: event.detail.value })}
              placeholder="填写答案，草稿自动保存在本机"
              className="w-full"
            />
          </div>
        )}
        {![0, 4].includes(p.kind) && (
          <Button disabled={!writable || busy} onClick={() => setConfirm(true)}>
            提交非编程题答卷
          </Button>
        )}
      </div>
    ) : null;
  const problemTotal = paper.problems.length;
  // 只有一道题时切换器没有意义，省掉那一行。
  const showSwitcher = problemTotal > 1;
  const problemOrdinal = paper.problems.findIndex((problem) => problemKey(problem) === pk) + 1;
  // 窄屏横条那一档没有常驻列表，触发按钮上带一个当前题目的判定胶囊。
  const currentStatus = p
    ? problemStatus(p, submissions.verdict, submissions.emptyVerdict)
    : undefined;
  return (
    <section className="oj-paper" ref={paperRef} data-oj-layout={layout}>
      <div className="oj-toolbar">
        <div>
          <h2>{paper.contest.title}</h2>
          <p className="oj-muted">
            <ContestStatus
              contest={paper.contest}
              now={now}
              complete={
                paper.problems.length > 0 &&
                completion?.total === paper.problems.length &&
                completion.accepted === completion.total
              }
            />
            <span>截止 {new Date(paper.contest.end).toLocaleString()}</span>
            {now >= paper.contest.begin && now < paper.contest.end && (
              <span className="oj-contest-countdown">
                （剩余 {formatRemaining(paper.contest.end - now)}）
              </span>
            )}
          </p>
        </div>
        <div className="oj-actions">
          <RankingTable siteId={site.id} paper={paper} />
          <Button
            variant="tonal"
            disabled={busy}
            onClick={() =>
              void action(async () => {
                await flush();
                await load();
              })
            }
          >
            刷新测验
          </Button>
          {paper.problems.some((p) => ![0, 4].includes(p.kind)) && (
            <Button disabled={!writable || busy} onClick={() => setConfirm(true)}>
              提交答卷
            </Button>
          )}
        </div>
      </div>
      {showContestDescription && (
        <Html html={paper.contest.description} base={paper.sourceUrl} siteId={site.id} />
      )}
      {error && <Banner status="warning">{error}</Banner>}
      {desktop && (
        <div className="oj-actions">
          <Button variant="tonal" disabled={busy} onClick={() => void send(true)}>
            发送全部题目到CPH
          </Button>
        </div>
      )}
      {/* 题目选择器：三档只渲染一档，档位由纸张容器的实测宽度决定（见 layout.ts）。 */}
      {layout === "strip" && showSwitcher && (
        <div className="oj-switcher-strip">
          <Tabs
            className="oj-problem-tabs"
            ref={(host) => nameTablist(host, "测验题目切换")}
            value={pk || undefined}
            onChange={(event) => {
              if (event.detail.value) void go({ problem: event.detail.value });
            }}
          >
            {paper.problems.map((problem) => {
              const status = problemStatus(problem, submissions.verdict, submissions.emptyVerdict);
              return (
                <Tab
                  value={problemKey(problem)}
                  key={problemKey(problem)}
                  icon={statusPillHtml(status.tone, status.text)}
                >
                  {problem.displayLabel}. {problem.title}
                </Tab>
              );
            })}
          </Tabs>
        </div>
      )}
      {layout === "sheet" && showSwitcher && (
        <div className="oj-switcher-compact">
          <Button
            className="oj-switcher-compact-trigger"
            variant="tonal"
            icon={currentStatus && statusPillHtml(currentStatus.tone, currentStatus.text)}
            onClick={() => setSheetOpen(true)}
          >
            题目 {problemOrdinal}/{problemTotal}
          </Button>
        </div>
      )}

      <div className="oj-workspace">
        {layout === "docked" && (
          <Drawer
            className="oj-problem-drawer"
            open
            headline="题目"
            position="start"
            width="240"
            value={pk}
            onChange={(event) => {
              if (event.detail.value) void go({ problem: event.detail.value });
            }}
          >
            <ProblemList
              problems={paper.problems}
              verdict={submissions.verdict}
              emptyVerdict={submissions.emptyVerdict}
            />
          </Drawer>
        )}
        <div className="oj-detail" ref={detailRef} data-oj-reading={reading ? "split" : "stacked"}>
          {p ? (
            <>
              <div className="oj-toolbar">
                <h2>
                  {p.displayLabel}. {p.title}
                </h2>
                <div className="oj-actions">
                  <Button
                    variant="tonal"
                    onClick={() =>
                      void action(async () => {
                        const markdown = problemMarkdown(p, paper);
                        if (!(await copyText(markdown))) setManualCopy(markdown);
                        else notifySuccess("完整题面 Markdown 已复制。");
                      })
                    }
                  >
                    复制 Markdown
                  </Button>
                  {desktop && [0, 4].includes(p.kind) && (
                    <Button variant="tonal" disabled={busy} onClick={() => void send()}>
                      发送到CPH
                    </Button>
                  )}
                </div>
              </div>
              <Tabs
                ref={(host) => nameTablist(host, "题目内容")}
                value={tab || undefined}
                onChange={(event) => {
                  if (event.detail.value) void go({ tab: event.detail.value });
                }}
              >
                <Tab value="statement">题面</Tab>
                <Tab value="answer">作答</Tab>
                <Tab value="records">提交记录</Tab>
              </Tabs>
              {tab === "statement" && (
                <div className="oj-reading">
                  <Statement p={p} paper={paper} siteId={site.id} />
                  <div className="oj-desktop-answer">{answerPanel}</div>
                </div>
              )}
              {tab === "answer" && answerPanel}
              {tab === "records" && recordPanel}
              {result && tab !== "records" && (
                <ResultAccordion
                  key={result.id}
                  submission={result}
                  polling={submissions.polling}
                  error={submissions.pollError}
                  onRefresh={() => submissions.show(result)}
                />
              )}
            </>
          ) : (
            <p>当前没有可见题目。</p>
          )}
        </div>
      </div>
      {layout === "sheet" &&
        showSwitcher &&
        typeof document !== "undefined" &&
        createPortal(
          <Drawer
            className="oj-problem-drawer"
            modal
            open={sheetOpen}
            headline="题目"
            position="start"
            width="280"
            value={pk}
            onClose={() => setSheetOpen(false)}
            onChange={(event) => {
              setSheetOpen(false);
              if (event.detail.value) void go({ problem: event.detail.value });
            }}
          >
            <ProblemList
              problems={paper.problems}
              verdict={submissions.verdict}
              emptyVerdict={submissions.emptyVerdict}
            />
          </Drawer>,
          document.body,
        )}
      {confirm &&
        typeof document !== "undefined" &&
        createPortal(
          <Dialog open={confirm} headline="确认提交答卷" onClose={() => setConfirm(false)}>
            <div className="space-y-3">
              <p>提交选择题、填空题和问答题；编程题需分别提交代码。</p>
              {[1, 2, 3].map((kind) => {
                const problems = paper.problems.filter((p) => p.kind === kind);
                const answered = problems.filter((p) => {
                  const a = drafts[problemKey(p)]?.answer;
                  return Array.isArray(a)
                    ? a.length > 0 && a.every((v) => String(v).trim())
                    : !!a?.trim();
                }).length;
                return (
                  <p key={kind}>
                    {kindName[kind]}：已作答 {answered} / {problems.length}
                  </p>
                );
              })}
              <p>以服务器允许的提交次数与截止时间为准。</p>
            </div>
            <div slot="actions">
              <Button variant="text" onClick={() => setConfirm(false)}>
                继续检查
              </Button>
              <Button disabled={busy || !writable} onClick={() => void submit(true)}>
                {busy ? "提交中…" : "确认提交"}
              </Button>
            </div>
          </Dialog>,
          document.body,
        )}
      {!!manualCopy &&
        typeof document !== "undefined" &&
        createPortal(
          <Dialog
            open={!!manualCopy}
            headline="手动复制 Markdown"
            onClose={() => setManualCopy(undefined)}
          >
            <TextField
              type="multiline"
              aria-label="完整 Markdown"
              readonly
              value={manualCopy ?? ""}
              className="w-full"
              ref={(host) => {
                host?.select();
              }}
            />
            <div slot="actions">
              <Button variant="tonal" onClick={() => setManualCopy(undefined)}>
                关闭
              </Button>
            </div>
          </Dialog>,
          document.body,
        )}
    </section>
  );
}
function plain(html: string) {
  const el = document.createElement("div");
  el.innerHTML = cleanHtml(html, "https://invalid.local/");
  return el.textContent ?? "";
}
