import { useCallback, useEffect } from "react";
import { listen } from "@tauri-apps/api/event";
import { useRouter } from "@tanstack/react-router";
import type { ActionPerformed } from "@tauri-apps/plugin-notification";
import { isChildDocument, useLogicalNavigation } from "../logical-navigation";
import { toast } from "../ui";
import { isOjNotice, resolveOjNoticeRoute } from "../oj/notice-route";
import { notificationSeverity } from "./categories";

export interface InAppNotificationPayload {
  category: string;
  title: string;
  body: string;
  route?: string | null;
  actionLabel?: string | null;
  extra?: unknown;
  id?: number | null;
  actionTypeId?: string | null;
}

/**
 * 应用内通知的唯一出口。
 *
 * 后端 `notification::notify` 已经按「设置 → 通知」的分类开关过滤：前台聚焦时
 * 发 `app://in-app-notification`，后台时发系统通知。这里统一负责两件事：
 * 1. 前台事件弹一个 Snackbar；
 * 2. 用户点击系统通知（或 Snackbar 动作）时，按 payload 里的路由跳转。
 *
 * 各业务模块不再各自挂 `onAction` 监听，避免同一通知被多个组件重复消费。
 */
export function NotificationCenter() {
  const navigation = useLogicalNavigation();
  const router = useRouter();

  const navigate = useCallback(
    (target: string) => {
      const targetPath = target.split(/[?#]/)[0];
      if (router.state.location.pathname === targetPath) {
        void navigation.replace(target);
      } else {
        void navigation.push(target);
      }
    },
    [navigation, router],
  );

  const openFromNotification = useCallback(
    (event: ActionPerformed) => {
      const extra = event.notification?.extra ?? {};

      const route = extra.route;
      if (typeof route === "string" && route.startsWith("/")) {
        navigate(route);
        return;
      }

      // 移动端排程的 OJ 提醒只带 ojNotice，需要按当前站点快照还原路由。
      if (isOjNotice(extra.ojNotice)) {
        void resolveOjNoticeRoute(extra.ojNotice).then((target) => {
          if (target) navigate(target);
        });
        return;
      }

      const course = extra.uoocCourse;
      if (typeof course === "string" && /^\d+$/.test(course)) {
        navigate(`/study/uooc?tab=tasks&course=${course}`);
        return;
      }

      if (
        event.notification?.actionTypeId === "electricity-low" ||
        event.actionId === "open-recharge" ||
        event.actionId === "electricity-low"
      ) {
        navigate("/life/campus-payments/recharge");
      }
    },
    [navigate],
  );

  useEffect(() => {
    if (typeof window === "undefined" || isChildDocument()) return;

    let active = true;
    const stops: (() => void)[] = [];

    if ("__TAURI_INTERNALS__" in window) {
      void listen<InAppNotificationPayload>("app://in-app-notification", (event) => {
        const payload = event.payload;
        if (!payload?.title) return;

        const show =
          notificationSeverity(payload.category, payload.title, payload.body) === "warning"
            ? toast.warning
            : toast.info;

        show(payload.title, {
          description: payload.body,
          actionLabel: payload.actionLabel ?? (payload.route ? "查看" : undefined),
          onAction: payload.route ? () => navigate(payload.route!) : undefined,
        });
      }).then((stop) => {
        if (active) stops.push(stop);
        else stop();
      });
    }

    void import("@tauri-apps/plugin-notification")
      .then(({ onAction }) => onAction(openFromNotification))
      .then((listener) => {
        if (active) stops.push(() => void listener.unregister());
        else void listener.unregister();
      })
      .catch(() => undefined);

    return () => {
      active = false;
      stops.forEach((stop) => stop());
    };
  }, [navigate, openFromNotification]);

  return null;
}
