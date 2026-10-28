import { useEffect, useMemo, useRef, useState } from "react";
import { useSearch } from "@tanstack/react-router";
import { Card, TextField, Button } from "material/react";
import { Chip } from "../ui";
import { LuPin, LuSearch, LuRefreshCw } from "react-icons/lu";
import { EmptyState } from "../components";
import { useCacheMode, canSelect } from "./view-mode";
import { useEnrollment } from "./use-enrollment";
import { Choice, OperationError, categories } from "./shared";
import { academicStatus, type Course } from "./model";
import { aggregateCourses, buildTree } from "./academic/tree";
import CourseActionDialog from "./CourseActionDialog";
export default function CatalogPage() {
  const cacheOnly = useCacheMode();
  const { snapshot: s, run, pending, errors } = useEnrollment(cacheOnly);
  const [cached, setCached] = useState<{
    courses: Course[];
    context: { student: string };
    total: number;
    complete: boolean;
    updatedAt: number | null;
    cachedPages: number;
    query: { category: string; keyword: string; page: number };
  }>();
  const shown = cacheOnly
    ? cached?.context.student === s?.context.student
      ? cached
      : undefined
    : s;
  const search = useSearch({ from: "/study/enrollment/" });
  const [keyword, setKeyword] = useState(search.keyword ?? s?.query.keyword ?? "");
  const [category, setCategory] = useState(s?.query.category ?? "TJKC");
  const [moreFilters, setMoreFilters] = useState(false);
  const [teacher, setTeacher] = useState("");
  const [time, setTime] = useState("");
  const [selection, setSelection] = useState<{ course: Course }>();
  const initial = useRef("");
  const formAccount = useRef("");
  const refresh = (page = 0) => {
    const query = { category, keyword, page };
    if (cacheOnly)
      void run<NonNullable<typeof cached>>({ type: "cache", query }).then((v) => v && setCached(v));
    else void run({ type: "refresh", query });
  };
  useEffect(() => {
    if (!s) return;
    const query = { category, keyword, page: 0 };
    if (formAccount.current !== s.context.student) {
      formAccount.current = s.context.student;
      query.category = s.query.category;
      query.keyword = search.keyword ?? s.query.keyword;
      setCategory(query.category);
      setKeyword(query.keyword);
    }
    const key = `${cacheOnly}:${s.context.student}`;
    if ((cacheOnly || canSelect(s)) && initial.current !== key) {
      initial.current = key;
      if (cacheOnly)
        void run<NonNullable<typeof cached>>({ type: "cache", query }).then(
          (v) => v && setCached(v),
        );
      else void run({ type: "refresh", query });
    }
  }, [s?.context.student, s?.loggedIn, s?.admission, cacheOnly]);
  const academicIndex = useMemo(
    () =>
      new Map(
        s?.academic
          ? aggregateCourses(buildTree(s.academic), "plan").map((c) => [c.code, c] as const)
          : [],
      ),
    [s?.academic?.updatedAt, s?.context.student],
  );
  const courses = [...(shown?.courses ?? [])]
    .filter((c) => c.teacher.includes(teacher) && c.schedule.includes(time))
    .sort((a, b) => Number(s?.pins.includes(b.id)) - Number(s?.pins.includes(a.id)));
  return (
    <div className="grid gap-4">
      <Card variant="outlined">
        <div className="grid gap-3 md:grid-cols-[180px_minmax(0,1fr)_auto] p-4">
          <Choice
            label="课程目录"
            value={category}
            onChange={setCategory}
            items={Object.entries(categories).map(([id, label]) => ({ id, label }))}
          />
          <TextField
            label="搜索课程"
            value={keyword}
            onChange={(e: any) => setKeyword(e.detail?.value ?? e.target?.value ?? "")}
            onKeyDown={(e: React.ChangeEvent<HTMLTextAreaElement> | any) =>
              e.key === "Enter" && refresh()
            }
            placeholder="课程名称或课程代码"
          />
          <Button
            className="self-end"
            disabled={!!(pending.refresh || pending.cache)}
            onClick={() => refresh()}
          >
            <LuSearch />
            {cacheOnly ? "检索缓存" : "查找课程"}
          </Button>
          <Button
            className="md:hidden"
            variant="text"
            aria-expanded={moreFilters}
            onClick={() => setMoreFilters((v) => !v)}
          >
            {moreFilters ? "收起筛选" : "更多筛选"}
          </Button>
          <div className={moreFilters ? "contents" : "hidden md:contents"}>
            <TextField
              label="本页教师"
              value={teacher}
              onChange={(e: any) => setTeacher(e.detail?.value ?? e.target?.value ?? "")}
              placeholder="不限教师"
            />
            <TextField
              label="本页上课时间"
              value={time}
              onChange={(e: any) => setTime(e.detail?.value ?? e.target?.value ?? "")}
              placeholder="例如 星期一"
            />
          </div>
        </div>
      </Card>
      <OperationError
        message={errors.refresh || errors.cache || errors.cacheSnapshot || errors.pin}
        onRetry={() => refresh(shown?.query.page ?? 0)}
      />
      <div className="flex flex-wrap items-center justify-between gap-2 text-sm text-muted">
        <span>
          {courses.length} 个教学班 ·{" "}
          {shown?.updatedAt
            ? `缓存更新于 ${new Date(shown.updatedAt).toLocaleString()}`
            : "尚未加载课程"}
        </span>
        <Button size="xs" variant="text" onClick={() => refresh(shown?.query.page ?? 0)}>
          <LuRefreshCw />
          {cacheOnly ? "重新读取缓存" : "刷新"}
        </Button>
      </div>
      {cacheOnly && (
        <p className="text-xs text-muted">
          已缓存 {cached?.total ?? 0} 个匹配教学班；仅检索已缓存内容，未缓存页面不会自动联网补齐。
        </p>
      )}
      <div className="grid gap-3 xl:grid-cols-2">
        {courses.map((c) => {
          const a = academicIndex.get(c.code);
          return (
            <Card
              key={c.id}
              variant="outlined"
              className="hover:shadow-lg hover:-translate-y-1 transition-all duration-200 flex flex-col justify-between"
            >
              <div className="flex items-start justify-between gap-3 p-4 pb-2">
                <div className="min-w-0">
                  <h3 className="font-semibold text-base">{c.name}</h3>
                  <p className="text-xs text-muted">
                    {c.code} · {c.teacher || "教师待定"}
                  </p>
                </div>
                <Button
                  disabled={cacheOnly}
                  size="xs"
                  variant={s?.pins.includes(c.id) ? "tonal" : "text"}
                  aria-label={`${s?.pins.includes(c.id) ? "取消" : ""}置顶 ${c.name}`}
                  onClick={() =>
                    void run({ type: "pin", id: c.id, pinned: !s?.pins.includes(c.id) })
                  }
                >
                  <LuPin />
                </Button>
              </div>
              <div className="grid gap-3 p-4 pt-0">
                <div className="flex flex-wrap gap-2">
                  <Chip size="sm" variant="soft">
                    {a ? a.credits.join("、") : (c.credits ?? "未知")} 学分
                  </Chip>
                  <Chip size="sm" variant="soft">
                    {a ? a.types.join(" / ") : (categories[c.category] ?? c.category)}
                  </Chip>
                  <Chip size="sm" variant="soft">
                    {a?.conflict ? "学业记录有差异" : academicStatus(a?.records[0])}
                  </Chip>
                </div>
                <p className="text-sm leading-relaxed text-muted">
                  {c.schedule || "上课时间尚未安排"}
                </p>
                {a && (
                  <p className="text-xs text-muted">
                    学业记录优先 · 选课系统记录 {c.credits ?? "未知"} 学分
                  </p>
                )}
              </div>
              <div slot="actions" className="flex items-center justify-between w-full">
                <span className="text-sm">
                  余量 {c.remaining ?? "待刷新"} / {c.capacity ?? "未知"}
                </span>
                <Button
                  size="xs"
                  disabled={cacheOnly || !canSelect(s)}
                  onClick={() => setSelection({ course: c })}
                >
                  选课 / 加入待抢
                </Button>
              </div>
            </Card>
          );
        })}
      </div>
      {!courses.length && (
        <Card variant="outlined">
          <div className="p-4">
            <EmptyState
              icon={<LuSearch className="size-10" aria-hidden="true" />}
              title={cacheOnly ? "未找到缓存课程" : "暂无匹配课程"}
              description={
                cacheOnly
                  ? "未缓存匹配课程；这不代表学校当前没有相关课程。"
                  : "调整目录和关键词后查询；连接不可用时仍可查看已有缓存。"
              }
              action={
                <Button size="xs" variant="tonal" onClick={() => refresh()}>
                  <LuRefreshCw />
                  重新查询
                </Button>
              }
            />
          </div>
        </Card>
      )}
      <div className="flex items-center justify-center gap-4">
        <Button
          variant="tonal"
          disabled={!!(pending.refresh || pending.cache) || !shown?.query.page}
          onClick={() => refresh((shown?.query.page ?? 1) - 1)}
        >
          上一页
        </Button>
        <span className="text-sm">第 {(shown?.query.page ?? 0) + 1} 页</span>
        <Button
          variant="tonal"
          disabled={!!(pending.refresh || pending.cache) || shown?.complete}
          onClick={() => refresh((shown?.query.page ?? 0) + 1)}
        >
          下一页
        </Button>
      </div>
      {!cacheOnly && (
        <CourseActionDialog selection={selection} onClose={() => setSelection(undefined)} />
      )}
    </div>
  );
}
