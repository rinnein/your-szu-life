/** 畅课（lms.szu.edu.cn）后端快照对应的 TypeScript 类型。 */

export interface LmsTodo {
  course_code: string;
  course_id: number;
  course_name: string;
  course_type: number;
  end_time: string;
  id: number;
  is_locked: boolean;
  is_student: boolean;
  prerequisites: unknown[];
  title: string;
  type: string;
}

export interface LmsNotificationPayload {
  activity_id?: number;
  activity_title?: string;
  activity_type?: string;
  course_id?: number;
  course_name?: string;
  created_at?: string;
  end_time?: string;
  homework_id?: number;
  homework_title?: string;
  submit_closed_time?: string;
  start_time?: string;
  uploads?: unknown[];
  [key: string]: unknown;
}

export interface LmsNotification {
  id: string;
  type: string;
  top: boolean;
  timestamp: number;
  payload: LmsNotificationPayload;
  unread: boolean;
}

export interface LmsProfile {
  uid: number;
  name: string;
  userNo: string;
}

export type LmsSessionState = "ready" | "needs_auth" | "unavailable";

export interface LmsSnapshot {
  session: LmsSessionState;
  message: string;
  profile: LmsProfile | null;
  todos: LmsTodo[];
  notifications: LmsNotification[];
  unreadCount: number;
  fetchedAt: number | null;
}
