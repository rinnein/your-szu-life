import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Button, Card, LoadingIndicator } from "material/react";
import { LuBell, LuCalendarClock, LuExternalLink, LuListTodo, LuRotateCw } from "react-icons/lu";
import { Banner, Chip, toast } from "../ui";
import { PageSurface } from "../shell/PageSurface";
import { TopAppBar } from "../shell/TopAppBar";
import { AuthGate } from "../shell/AuthGate";
import type { LmsNotification, LmsSnapshot, LmsTodo } from "./types";
import { LmsTodoLinkDialog, openLmsTodoLink, type LmsTodoLink } from "./link";

const NOTIFICATION_TYPE_LABELS: Record<string, string> = {
  activity_expiring: "活动即将截止",
  homework_expiring: "作业即将截止",
  homework_expiring_today: "作业今日截止",
  activity_opened: "新内容发布",
};

function notificationTypeLabel(type: string): string {
  return NOTIFICATION_TYPE_LABELS[type] ?? type;
}

function notificationTitle(notification: LmsNotification): string {
  return (
    notification.payload.activity_title ??
    notification.payload.homework_title ??
    notification.payload.course_name ??
    "（无标题）"
  );
}

function formatIsoTime(raw: string | undefined): string {
  if (!raw) return "—";
  const date = new Date(raw);
  return isNaN(date.getTime()) ? raw : date.toLocaleString("zh-CN", { hour12: false });
}

function formatTimestamp(ms: number): string {
  return new Date(ms).toLocaleString("zh-CN", { hour12: false });
}

function formatDeadline(raw: string): string {
  const date = new Date(raw);
  if (isNaN(date.getTime())) return raw;
  const diffMs = date.getTime() - Date.now();
  if (diffMs < 0) return `${formatIsoTime(raw)}（已截止）`;
  const hours = Math.ceil(diffMs / 3_600_000);
  if (hours <= 24) return `${formatIsoTime(raw)}（${hours} 小时内截止）`;
  return formatIsoTime(raw);
}

export default function LmsPage() {
  const [snapshot, setSnapshot] = useState<LmsSnapshot | null>(null);
  const [loading, setLoading] = useState(true);
  const [refreshing, setRefreshing] = useState(false);
  const [link, setLink] = useState<LmsTodoLink | null>(null);

  const load = useCallback(async (force: boolean) => {
    if (force) setRefreshing(true);
    else setLoading(true);
    try {
      setSnapshot(await invoke<LmsSnapshot>(force ? "refresh_lms" : "get_lms_snapshot"));
    } catch (error) {
      toast.danger("获取畅课数据失败", { description: String(error) });
    } finally {
      setLoading(false);
      setRefreshing(false);
    }
  }, []);

  useEffect(() => {
    void load(false);
  }, [load]);

  const todos = [...(snapshot?.todos ?? [])].sort(
    (a, b) => new Date(a.end_time).getTime() - new Date(b.end_time).getTime(),
  );
  const notifications = snapshot?.notifications ?? [];
  const needsAuth = snapshot?.session === "needs_auth";
  const unavailable = snapshot?.session === "unavailable";

  const handleTodoClick = (todo: LmsTodo) => {
    void openLmsTodoLink(todo, setLink);
  };

  return (
    <AuthGate>
      <PageSurface maxWidth="2xl">
        <TopAppBar title="畅课" description="荔园畅课（lms.szu.edu.cn）的课程待办与站内通知" />

        {needsAuth && (
          <Banner status="warning">
            <Banner.Title>尚未获取到畅课会话</Banner.Title>
            <Banner.Description>
              {snapshot?.message || "请先在应用内完成深圳大学统一认证，然后点击刷新。"}
            </Banner.Description>
          </Banner>
        )}
        {unavailable && (
          <Banner status="warning">
            <Banner.Title>畅课数据暂时不可用</Banner.Title>
            <Banner.Description>
              {snapshot?.message || "网络异常，稍后可点击刷新重试。"}
            </Banner.Description>
          </Banner>
        )}

        <section aria-labelledby="lms-todos-heading" className="mb-8">
          <div className="flex items-center justify-between gap-4 mb-3">
            <h2 id="lms-todos-heading" className="text-lg font-medium flex items-center gap-2">
              <LuListTodo className="text-primary" aria-hidden="true" />
              <span>课程待办</span>
              <Chip size="sm" color="default" variant="soft">
                {todos.length}
              </Chip>
            </h2>
            <Button
              variant="text"
              className="text-xs"
              disabled={loading || refreshing}
              onClick={() => void load(true)}
              aria-label="刷新畅课数据"
            >
              {refreshing ? <LoadingIndicator size={16} /> : <LuRotateCw aria-hidden="true" />}
              刷新
            </Button>
          </div>

          {loading ? (
            <div className="py-8 text-center text-sm text-muted flex items-center justify-center gap-2">
              <LoadingIndicator size={16} /> 正在加载…
            </div>
          ) : todos.length === 0 ? (
            <div className="py-8 text-center text-sm text-muted">
              {needsAuth || unavailable ? "会话恢复后即可查看待办" : "当前没有未完成的课程待办"}
            </div>
          ) : (
            <div className="flex flex-col gap-2.5">
              {todos.map((todo) => (
                <Card key={`${todo.course_id}:${todo.id}`} variant="outlined" className="p-3">
                  <button
                    type="button"
                    onClick={() => handleTodoClick(todo)}
                    className="w-full text-left flex items-start justify-between gap-3 group"
                    aria-label={`打开畅课任务：${todo.title}`}
                  >
                    <div className="flex-1 min-w-0">
                      <div className="flex items-center gap-2 mb-1">
                        <Chip size="sm" color="info" variant="soft">
                          畅课
                        </Chip>
                        <span className="text-xs text-muted truncate">{todo.course_name}</span>
                        {todo.is_locked && (
                          <Chip size="sm" color="danger" variant="soft">
                            已锁定
                          </Chip>
                        )}
                      </div>
                      <div className="text-sm font-medium text-foreground truncate group-hover:text-primary transition-colors">
                        {todo.title}
                      </div>
                      <div className="text-xs text-muted flex items-center gap-1 mt-1">
                        <LuCalendarClock aria-hidden="true" />
                        <span>截止：{formatDeadline(todo.end_time)}</span>
                      </div>
                    </div>
                    <LuExternalLink
                      className="text-muted group-hover:text-primary transition-colors mt-1 shrink-0"
                      aria-hidden="true"
                    />
                  </button>
                </Card>
              ))}
            </div>
          )}
        </section>

        <section aria-labelledby="lms-notifications-heading" className="mb-8">
          <div className="flex items-center gap-2 mb-3">
            <h2
              id="lms-notifications-heading"
              className="text-lg font-medium flex items-center gap-2"
            >
              <LuBell className="text-primary" aria-hidden="true" />
              <span>最近通知</span>
              {(snapshot?.unreadCount ?? 0) > 0 && (
                <Chip size="sm" color="danger" variant="soft">
                  {snapshot?.unreadCount} 条未读
                </Chip>
              )}
            </h2>
          </div>

          {loading ? null : notifications.length === 0 ? (
            <div className="py-8 text-center text-sm text-muted">
              {needsAuth || unavailable ? "会话恢复后即可查看通知" : "暂无站内通知"}
            </div>
          ) : (
            <div className="flex flex-col gap-2.5">
              {notifications.map((notification) => (
                <Card key={notification.id} variant="outlined" className="p-3">
                  <div className="flex items-center gap-2 mb-1">
                    <Chip
                      size="sm"
                      color={notification.unread ? "accent" : "default"}
                      variant="soft"
                    >
                      {notificationTypeLabel(notification.type)}
                    </Chip>
                    {notification.payload.course_name && (
                      <span className="text-xs text-muted truncate">
                        {notification.payload.course_name}
                      </span>
                    )}
                  </div>
                  <div className="text-sm font-medium text-foreground">
                    {notificationTitle(notification)}
                  </div>
                  <div className="text-xs text-muted mt-1">
                    {formatTimestamp(notification.timestamp)}
                  </div>
                </Card>
              ))}
            </div>
          )}
        </section>

        <LmsTodoLinkDialog link={link} onClose={() => setLink(null)} />
      </PageSurface>
    </AuthGate>
  );
}
