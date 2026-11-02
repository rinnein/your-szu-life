import { useCallback, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useChannelSubscription } from "../tauri-channel";
import type { ExamTask, HomeworkTask } from "./types";

export type UoocAssessments = {
  exams: ExamTask[];
  homeworks: HomeworkTask[];
};

const EMPTY: UoocAssessments = { exams: [], homeworks: [] };

type AssessmentsEvent = { type: "initial" | "snapshot"; data: UoocAssessments };

/**
 * 订阅后端缓存的考试与作业列表。
 *
 * 列表来自远端课程接口，由后端统一刷新并经 Channel 推送；前端只在用户
 * 主动点击“刷新列表”时请求一次刷新，不再自己按时间轮询。
 */
export function useUoocAssessments() {
  const [data, setData] = useState<UoocAssessments>(EMPTY);
  const [ready, setReady] = useState(false);
  const [refreshing, setRefreshing] = useState(false);

  useChannelSubscription<AssessmentsEvent>("subscribe_uooc_assessments", (event) => {
    if (!event?.data) return;
    setData({
      exams: event.data.exams ?? [],
      homeworks: event.data.homeworks ?? [],
    });
    setReady(true);
  });

  const refresh = useCallback(async () => {
    setRefreshing(true);
    try {
      await invoke("refresh_uooc_assessments");
    } finally {
      setRefreshing(false);
    }
  }, []);

  return { exams: data.exams, homeworks: data.homeworks, ready, refreshing, refresh };
}
