import { useEffect } from "react";
import { listen } from "@tauri-apps/api/event";
import { useRouter } from "@tanstack/react-router";
import { useLogicalNavigation } from "../logical-navigation";

export function QuizNotifications() {
  const navigation = useLogicalNavigation();
  const router = useRouter();
  useEffect(() => {
    if (!("__TAURI_INTERNALS__" in window)) return;
    let live = true;
    const dispose: (() => void)[] = [];
    const open = (cid: unknown) => {
      if (!live || typeof cid !== "string" || !/^\d+$/.test(cid)) return;
      const href = `/uooc?tab=tasks&course=${cid}`;
      void (router.state.location.pathname === "/uooc"
        ? navigation.replace(href)
        : navigation.push(href));
    };
    void listen<string>("uooc-quiz-open", (e) => open(e.payload))
      .then((stop) => {
        if (live) dispose.push(stop);
        else stop();
      })
      .catch(() => {});
    void import("@tauri-apps/plugin-notification")
      .then(({ onAction }) =>
        onAction((event) => open(event.notification?.extra?.uoocCourse)),
      )
      .then((listener) => {
        if (live) dispose.push(() => void listener.unregister());
        else void listener.unregister();
      })
      .catch(() => {});
    return () => {
      live = false;
      dispose.forEach((fn) => fn());
    };
  }, [navigation, router]);
  return null;
}
