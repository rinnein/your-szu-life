import { invoke } from "@tauri-apps/api/core";

/** 会话状态（与后端 SessionState 对应）。 */
export type JxxkSessionState = "ready" | "needsAuth" | "unavailable";

export interface JxxkSnapshot {
  session: JxxkSessionState;
  message: string;
  semesters: JxxkSemester[];
  fetchedAt?: number | null;
}

export interface JxxkSemester {
  id: string;
  name: string;
  beginDate?: string | null;
  endDate?: string | null;
}

export interface JxxkCourse {
  id: string;
  name: string;
  teacher?: string | null;
  yearSemesterId?: string | null;
  yearSemesterName?: string | null;
  categorySectionName?: string | null;
  clazzes?: string | null;
  courseResourceCount?: number | null;
}

export interface JxxkCoursePage {
  pageIndex: number;
  pageSize: number;
  recordCount: number;
  content: JxxkCourse[];
}

/** 直播状态：1=未开始 2=直播中 3=已结束 4=可看回放。 */
export type JxxkLiveState = 1 | 2 | 3 | 4;

export interface JxxkLesson {
  id: string;
  targetName: string;
  liveId?: string | null;
  liveState?: JxxkLiveState | null;
  liveStateText?: string | null;
  startTime?: string | null;
  endTime?: string | null;
  classroom?: string | null;
  mediaVideoId?: string | null;
}

export interface JxxkLessonPage {
  pageIndex: number;
  pageSize: number;
  recordCount: number;
  content: JxxkLesson[];
}

/** 巡课页面的缓存快照：学期 + 课程 + 课次。 */
export interface JxxkState {
  snapshot: JxxkSnapshot;
  courses: Record<string, JxxkCoursePage>;
  lessons: Record<string, JxxkLessonPage>;
}

export interface JxxkVideoStream {
  id: number;
  name: string;
  streamNo?: string | null;
  streamLabel?: string | null;
  fileSize?: number | null;
  duration?: number | null;
  fileName?: string | null;
}

export interface JxxkLessonStreams {
  videoId: string;
  mediaVideoId: string;
  title: string;
  duration?: number | null;
  download: boolean;
  recordEquipmentName?: string | null;
  streams: JxxkVideoStream[];
}

export interface JxxkPlayUrlScreen {
  screenTag?: string | null;
  streamName?: string | null;
  streamNo?: string | null;
  streamLabel?: string | null;
  streamPermissionType?: string | null;
  playUrl: string;
}

export interface JxxkDownloadUrl {
  streamName?: string | null;
  streamNo?: string | null;
  streamLabel?: string | null;
  fileName?: string | null;
  fileUrl: string;
}

export interface JxxkLiveStreamSource {
  url: string;
  label?: string | null;
}

export type JxxkDownloadTaskStatus =
  | "preparing"
  | "downloading"
  | "paused"
  | "completed"
  | "failed"
  | "cancelled";

export interface JxxkDownloadTask {
  id: string;
  courseName: string;
  lessonName: string;
  streamName: string;
  fileName: string;
  status: JxxkDownloadTaskStatus;
  downloaded: number;
  total: number;
  speed: number;
  error?: string | null;
  createdAt: number;
  /** aria2 下载明细：供 DownloadSession 卡片展示分片位图。 */
  port?: number;
  numPieces?: number;
  pieceLength?: number;
  bitfield?: string | null;
  connections?: number;
  isResumed?: boolean;
  resumedBytes?: number;
  totalActiveTasks?: number;
}

export async function getSnapshot(): Promise<JxxkSnapshot> {
  return invoke<JxxkSnapshot>("get_jxxk_snapshot");
}

export async function refreshSnapshot(): Promise<JxxkSnapshot> {
  return invoke<JxxkSnapshot>("refresh_jxxk");
}

export async function listCourses(
  yearSemesterId: string | null,
  searchKey: string | null,
): Promise<JxxkCoursePage> {
  return invoke<JxxkCoursePage>("jxxk_list_courses", {
    yearSemesterId: yearSemesterId ?? "",
    searchKey: searchKey ?? "",
    pageIndex: 1,
    pageSize: 200,
  });
}

export async function listLessons(
  courseId: string,
  pageIndex = 1,
  pageSize = 100,
): Promise<JxxkLessonPage> {
  return invoke<JxxkLessonPage>("jxxk_list_lessons", { courseId, pageIndex, pageSize });
}

export async function lessonStreams(liveId: string): Promise<JxxkLessonStreams> {
  return invoke<JxxkLessonStreams>("jxxk_lesson_streams", { liveId });
}

export async function playUrls(liveId: string): Promise<JxxkPlayUrlScreen[]> {
  return invoke<JxxkPlayUrlScreen[]>("jxxk_play_urls", { liveId });
}

export async function liveStream(liveId: string): Promise<JxxkLiveStreamSource[]> {
  return invoke<JxxkLiveStreamSource[]>("jxxk_live_stream", { liveId });
}

export async function downloadUrls(mediaVideoId: string): Promise<JxxkDownloadUrl[]> {
  return invoke<JxxkDownloadUrl[]>("jxxk_download_urls", { mediaVideoId });
}

/** 启动一个录像下载任务（后台执行），返回任务 id。 */
export async function invokeDownload(options: {
  fileUrl: string;
  fileName: string;
  semesterName: string;
  courseName: string;
  teacherName: string;
  lessonName: string;
  streamName: string;
  streamNo: string | null;
  mediaVideoId: string;
  expectedSize: number | null;
  subtitleVtt: string | null;
}): Promise<string> {
  return invoke<string>("jxxk_download", {
    fileUrl: options.fileUrl,
    fileName: options.fileName,
    semesterName: options.semesterName,
    courseName: options.courseName,
    teacherName: options.teacherName,
    lessonName: options.lessonName,
    streamName: options.streamName,
    streamNo: options.streamNo,
    mediaVideoId: options.mediaVideoId,
    expectedSize: options.expectedSize,
    subtitleVtt: options.subtitleVtt,
  });
}

export async function listDownloads(): Promise<JxxkDownloadTask[]> {
  return invoke<JxxkDownloadTask[]>("jxxk_download_list");
}

export async function pauseDownload(taskId: string): Promise<void> {
  await invoke("jxxk_download_pause", { taskId });
}

export async function resumeDownload(taskId: string): Promise<void> {
  await invoke("jxxk_download_resume", { taskId });
}

export async function removeDownload(taskId: string, deleteFile: boolean): Promise<void> {
  await invoke("jxxk_download_remove", { taskId, deleteFile });
}

/** 拉取一节课的平台生成字幕（WebVTT 文本）。 */
export async function subtitle(mediaVideoId: string): Promise<string> {
  return invoke<string>("jxxk_subtitle", { mediaVideoId });
}

/**
 * 把巡课域的媒体地址包装为应用内流代理地址（jxxk-stream://），
 * 由 Rust 侧携带会话回源取流，规避跨域与混合内容限制。
 */
export function toProxyUrl(target: string): string {
  const bytes = new TextEncoder().encode(target);
  let binary = "";
  bytes.forEach((b) => {
    binary += String.fromCharCode(b);
  });
  const encoded = btoa(binary).replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/, "");
  return `jxxk-stream://stream/${encoded}`;
}

/** 从 "2026-09-04 10:15:00" 之类的时间串取星期几（1=周一 … 7=周日），无效返回 null。 */
export function weekdayOf(raw: string | null | undefined): number | null {
  if (!raw) return null;
  const date = new Date(raw.replace(" ", "T"));
  if (isNaN(date.getTime())) return null;
  return date.getDay() === 0 ? 7 : date.getDay();
}

/** 简单并发限制的 map，避免同时给平台发几十个请求。 */
export async function mapWithConcurrency<T, R>(
  items: T[],
  limit: number,
  fn: (item: T) => Promise<R>,
  onEach?: () => void,
): Promise<R[]> {
  const results: R[] = Array.from({ length: items.length });
  let next = 0;
  const workers = Array.from({ length: Math.min(limit, items.length) }, async () => {
    while (next < items.length) {
      const index = next++;
      results[index] = await fn(items[index]);
      onEach?.();
    }
  });
  await Promise.all(workers);
  return results;
}
