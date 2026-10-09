import { lazy, Suspense, useCallback, useEffect, useMemo, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useSelector } from "@tanstack/react-store";
import { ojProgressCache, progressScope, progressScopeKey } from "./progress-store";
import {
  Alert,
  Button,
  Checkbox,
  CheckboxGroup,
  Chip,
  Input,
  Label,
  ListBox,
  Modal,
  Radio,
  RadioGroup,
  Select,
  Spinner,
  Tabs,
  TextArea,
  TextField,
} from "@heroui/react";
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
import { Verdict } from "./JudgeResult";
import { SubmissionRecord, ResultAccordion } from "./SubmissionRecord";
import { RankingTable } from "./RankingTable";
import { ContestStatus, formatRemaining } from "./ContestTable";
import { useSubmissions } from "./useSubmissions";
import { verdicts } from "./judging";
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
        {busy && <Spinner aria-label="加载测验" />}
        {error && (
          <Alert status="warning">
            <Alert.Content>{error}</Alert.Content>
          </Alert>
        )}
        <Button variant="secondary" onPress={() => void action(load)}>
          重新加载
        </Button>
      </>
    );
  const recordPanel = (
    <div className="oj-form">
      <Button variant="secondary" isDisabled={busy} onPress={() => void action(refreshRecords)}>
        刷新提交记录
      </Button>
      {submissions.recordError && (
        <Alert status="warning">
          <Alert.Content>{submissions.recordError}</Alert.Content>
        </Alert>
      )}
      {!records && !submissions.recordError && <Spinner aria-label="正在读取当前题目提交记录" />}
      {records && records.records.length === 0 && (
        <p className="oj-muted">当前题目暂无提交记录。</p>
      )}
      {records?.uncertain.map((u) => (
        <Alert key={u.key} status="warning">
          <Alert.Content>
            <Alert.Title>上次提交结果待确认</Alert.Title>
            <Alert.Description>
              {new Date(u.details.time).toLocaleString()}
              。请先核对以下记录，再确认是否需要重新提交。
            </Alert.Description>
            <Button
              variant="secondary"
              onPress={() =>
                void action(async () => {
                  await invoke("acknowledge_oj_submission", { siteId: site.id, key: u.key });
                  await refreshRecords();
                })
              }
            >
              已核对，解除待确认状态
            </Button>
          </Alert.Content>
        </Alert>
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
          <Alert status="warning">
            <Alert.Content>
              <Alert.Title>cph-ng 代码已缓存，编辑器另有未保存修改</Alert.Title>
              <Alert.Description>当前编辑内容已保留，请选择继续使用哪一份代码。</Alert.Description>
              <div className="oj-actions">
                <Button
                  variant="secondary"
                  isDisabled={busy}
                  onPress={() =>
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
                  variant="secondary"
                  isDisabled={busy}
                  onPress={() =>
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
            </Alert.Content>
          </Alert>
        )}
        {!writable && (
          <Alert>
            <Alert.Content>当前测验不可提交，已保存草稿仍可查看。</Alert.Content>
          </Alert>
        )}
        {[0, 4].includes(p.kind) ? (
          <>
            <p className="oj-muted">语言和编译选项保存于当前 OJ 实例，切换题目时沿用。</p>
            {!p.languages.includes(draft.language) && (
              <Alert status="warning">
                <Alert.Content>当前题目不支持已保存的语言，请手动选择支持的语言。</Alert.Content>
              </Alert>
            )}
            <Select
              aria-label="编程语言"
              value={draft.language || null}
              onChange={(key) =>
                void action(async () => {
                  const language = String(key);
                  await savePreferences(
                    language,
                    compileOptions(language, preferencesRef.current.optionsByLanguage[language]),
                  );
                })
              }
              isDisabled={!writable || busy}
            >
              <Label>语言</Label>
              <Select.Trigger>
                <Select.Value />
                <Select.Indicator />
              </Select.Trigger>
              <Select.Popover>
                <ListBox>
                  {[...new Set([draft.language, ...p.languages])]
                    .filter(Boolean)
                    .map((language) => (
                      <ListBox.Item
                        key={language}
                        id={language}
                        isDisabled={!p.languages.includes(language)}
                        textValue={languageName[language] ?? language}
                      >
                        {languageName[language] ?? language}
                        <ListBox.ItemIndicator />
                      </ListBox.Item>
                    ))}
                </ListBox>
              </Select.Popover>
            </Select>
            <CompileOptions
              language={draft.language}
              value={draft.options}
              disabled={!writable || busy}
              onChange={(options) => void action(() => savePreferences(draft.language, options))}
            />
            {desktop ? (
              <Suspense fallback={<Spinner aria-label="加载桌面编辑器" />}>
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
              isDisabled={
                busy ||
                submissions.polling ||
                !writable ||
                !!incomingDrafts[pk] ||
                !draft.code.trim() ||
                !p.languages.includes(draft.language)
              }
              onPress={() => void submit(false)}
            >
              提交代码
            </Button>
          </>
        ) : p.kind === 1 ? (
          p.multiple ? (
            <CheckboxGroup
              aria-label="选择答案"
              value={Array.isArray(draft.answer) ? draft.answer.map(String) : []}
              onChange={(values) => edit({ answer: values.map(Number) })}
              isDisabled={!writable}
            >
              <Label>多选</Label>
              {p.choices.map((c, i) => (
                <Checkbox value={String(i)} key={i}>
                  <Checkbox.Content>
                    <Checkbox.Control>
                      <Checkbox.Indicator />
                    </Checkbox.Control>
                    {String.fromCharCode(65 + i)}. {plain(c)}
                  </Checkbox.Content>
                </Checkbox>
              ))}
            </CheckboxGroup>
          ) : (
            <RadioGroup
              aria-label="选择答案"
              value={Array.isArray(draft.answer) ? String(draft.answer[0] ?? "") : ""}
              onChange={(value) => edit({ answer: [Number(value)] })}
              isDisabled={!writable}
            >
              <Label>单选</Label>
              {p.choices.map((c, i) => (
                <Radio key={i} value={String(i)}>
                  <Radio.Content>
                    <Radio.Control>
                      <Radio.Indicator />
                    </Radio.Control>
                    {String.fromCharCode(65 + i)}. {plain(c)}
                  </Radio.Content>
                </Radio>
              ))}
            </RadioGroup>
          )
        ) : p.kind === 2 ? (
          Array.from({ length: p.blanks }, (_, i) => (
            <TextField
              key={i}
              isDisabled={!writable}
              value={String(Array.isArray(draft.answer) ? (draft.answer[i] ?? "") : "")}
              onChange={(value) => {
                const values = Array.from({ length: p.blanks }, (_, j) =>
                  String(Array.isArray(draft.answer) ? (draft.answer[j] ?? "") : ""),
                );
                values[i] = value;
                edit({ answer: values });
              }}
            >
              <Label>第 {i + 1} 空</Label>
              <Input />
            </TextField>
          ))
        ) : (
          <TextField
            isDisabled={!writable}
            value={typeof draft.answer === "string" ? draft.answer : ""}
            onChange={(answer) => edit({ answer })}
          >
            <Label>问答题作答</Label>
            <TextArea rows={12} placeholder="填写答案，草稿自动保存在本机" />
          </TextField>
        )}
        {![0, 4].includes(p.kind) && (
          <Button isDisabled={!writable || busy} onPress={() => setConfirm(true)}>
            提交非编程题答卷
          </Button>
        )}
      </div>
    ) : null;
  return (
    <section className="oj-paper">
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
          <Button
            variant="secondary"
            isDisabled={busy}
            onPress={() =>
              void action(async () => {
                await flush();
                await load();
              })
            }
          >
            刷新测验
          </Button>
          {paper.problems.some((p) => ![0, 4].includes(p.kind)) && (
            <Button isDisabled={!writable || busy} onPress={() => setConfirm(true)}>
              提交答卷
            </Button>
          )}
        </div>
      </div>
      {showContestDescription && (
        <Html html={paper.contest.description} base={paper.sourceUrl} siteId={site.id} />
      )}
      {error && (
        <Alert status="warning">
          <Alert.Content>{error}</Alert.Content>
        </Alert>
      )}
      {desktop && (
        <div className="oj-actions">
          <Button variant="secondary" isDisabled={busy} onPress={() => void send(true)}>
            发送全部题目到CPH
          </Button>
        </div>
      )}
      <RankingTable siteId={site.id} paper={paper} />
      <div className="oj-workspace">
        <aside className="oj-directory">
          <h3>题目</h3>
          {[0, 1, 2, 3, 4]
            .filter((kind) => paper.problems.some((p) => p.kind === kind))
            .map((kind) => (
              <div className="oj-problem-group" key={kind}>
                <h4 className="flex flex-wrap items-center gap-2">
                  {kindName[kind]}
                  {[0, 4].includes(kind) && (
                    <span>
                      {
                        paper.problems.filter(
                          (p) => p.kind === kind && submissions.verdict(p) === 0,
                        ).length
                      }
                      /{paper.problems.length}
                    </span>
                  )}
                  {paper.problems
                    .filter((p) => p.kind === kind)
                    .every((p) => submissions.verdict(p) === 0) && (
                    <Chip size="sm" variant="primary" color="success">
                      已完成
                    </Chip>
                  )}
                </h4>
                {paper.problems
                  .filter((p) => p.kind === kind)
                  .map((problem) => (
                    <Button
                      className="oj-directory-item"
                      data-verdict-tone={
                        verdicts[submissions.verdict(problem) ?? 99]?.color ?? "default"
                      }
                      data-selected={problemKey(problem) === pk}
                      key={problemKey(problem)}
                      variant={problemKey(problem) === pk ? "secondary" : "tertiary"}
                      onPress={() => void go({ problem: problemKey(problem) })}
                    >
                      <span>
                        {problem.displayLabel}. {problem.title}
                      </span>
                      <Verdict
                        status={submissions.verdict(problem)}
                        empty={submissions.emptyVerdict(problem)}
                      />
                    </Button>
                  ))}
              </div>
            ))}
        </aside>
        <div className="oj-detail">
          {p ? (
            <>
              <div className="oj-toolbar">
                <h2>
                  {p.displayLabel}. {p.title}
                </h2>
                <div className="oj-actions">
                  <Button
                    variant="secondary"
                    onPress={() =>
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
                    <Button variant="secondary" isDisabled={busy} onPress={() => void send()}>
                      发送到CPH
                    </Button>
                  )}
                </div>
              </div>
              <Tabs selectedKey={tab} onSelectionChange={(key) => void go({ tab: String(key) })}>
                <Tabs.ListContainer>
                  <Tabs.List aria-label="题目内容">
                    <Tabs.Tab id="statement">
                      题面
                      <Tabs.Indicator />
                    </Tabs.Tab>
                    <Tabs.Tab id="answer">
                      作答
                      <Tabs.Indicator />
                    </Tabs.Tab>
                    <Tabs.Tab id="records">
                      提交记录
                      <Tabs.Indicator />
                    </Tabs.Tab>
                  </Tabs.List>
                </Tabs.ListContainer>
                <Tabs.Panel id="statement">
                  <div className="oj-reading">
                    <Statement p={p} paper={paper} siteId={site.id} />
                    <div className="oj-desktop-answer">{answerPanel}</div>
                  </div>
                </Tabs.Panel>
                <Tabs.Panel id="answer">{answerPanel}</Tabs.Panel>
                <Tabs.Panel id="records">{recordPanel}</Tabs.Panel>
              </Tabs>
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
      <Modal isOpen={confirm} onOpenChange={setConfirm}>
        <Modal.Backdrop>
          <Modal.Container>
            <Modal.Dialog>
              <Modal.CloseTrigger />
              <Modal.Header>
                <Modal.Heading>确认提交答卷</Modal.Heading>
              </Modal.Header>
              <Modal.Body>
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
              </Modal.Body>
              <Modal.Footer>
                <Button variant="tertiary" onPress={() => setConfirm(false)}>
                  继续检查
                </Button>
                <Button isDisabled={busy || !writable} onPress={() => void submit(true)}>
                  {busy ? "提交中…" : "确认提交"}
                </Button>
              </Modal.Footer>
            </Modal.Dialog>
          </Modal.Container>
        </Modal.Backdrop>
      </Modal>
      <Modal isOpen={!!manualCopy} onOpenChange={(open) => !open && setManualCopy(undefined)}>
        <Modal.Backdrop>
          <Modal.Container>
            <Modal.Dialog>
              <Modal.CloseTrigger />
              <Modal.Header>
                <Modal.Heading>手动复制 Markdown</Modal.Heading>
              </Modal.Header>
              <Modal.Body>
                <TextArea
                  aria-label="完整 Markdown"
                  readOnly
                  value={manualCopy ?? ""}
                  rows={18}
                  onFocus={(e) => e.currentTarget.select()}
                />
              </Modal.Body>
            </Modal.Dialog>
          </Modal.Container>
        </Modal.Backdrop>
      </Modal>
    </section>
  );
}
function plain(html: string) {
  const el = document.createElement("div");
  el.innerHTML = cleanHtml(html, "https://invalid.local/");
  return el.textContent ?? "";
}
