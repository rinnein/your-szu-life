/**
 * SZU（实验性）签到面板：当天课次列表 + 雷达定位/照片签到模态框。
 *
 * 照片来源：Android 用带 capture 的文件选择（系统相机/相册），桌面用普通
 * 文件选择；照片在 Rust 侧压缩归一为 JPEG，并按课程记忆上次照片以便
 * 下次免拍直接提交。
 */
import { useCallback, useEffect, useRef, useState } from "react";
import { Button, Card, Dialog, LoadingIndicator } from "material/react";
import { LuCamera, LuImageUp, LuRefreshCw } from "react-icons/lu";
import * as tauriCore from "@tauri-apps/api/core";
import { Banner, toast } from "../ui";
import { isAndroidPlatform } from "../os-info";
import type { Building, LastPhotoView, ResolvedPoint, TimetableSession } from "./types";
import { formatSessionTime, localDateKey } from "./types";
import { RadarForm, type PointSelection } from "./RadarForm";

interface SzuPanelProps {
  buildings: Building[];
}

export function SzuPanel({ buildings }: SzuPanelProps) {
  const [sessions, setSessions] = useState<TimetableSession[]>([]);
  const [loading, setLoading] = useState(true);
  const [refreshing, setRefreshing] = useState(false);
  const [active, setActive] = useState<TimetableSession | null>(null);
  const [qwState, setQwState] = useState<"ready" | "needs_auth" | "unavailable">("ready");
  const [qwMessage, setQwMessage] = useState("");

  const load = useCallback(async (force: boolean) => {
    if (force) setRefreshing(true);
    else setLoading(true);
    try {
      const status = await tauriCore.invoke<{ state: typeof qwState; message: string }>(
        "signin_get_qw_status",
        { force },
      );
      setQwState(status.state);
      setQwMessage(status.message ?? "");
    } catch {
      // 状态查询失败不阻塞课表展示。
    }
    try {
      const cache = await tauriCore.invoke<{ sessions: TimetableSession[] }>(
        "signin_get_timetable",
        { force },
      );
      const today = localDateKey();
      setSessions(cache.sessions.filter((session) => session.date === today));
    } catch (error) {
      toast.danger("获取课表缓存失败", { description: String(error) });
    } finally {
      setLoading(false);
      setRefreshing(false);
    }
  }, []);

  useEffect(() => {
    void load(false);
  }, [load]);

  return (
    <section aria-label="SZU 考勤课次列表" className="flex flex-col gap-3">
      {qwState === "needs_auth" && (
        <Banner status="warning">
          <Banner.Title>尚未获取到 SZU 考勤会话</Banner.Title>
          <Banner.Description>
            {qwMessage || "请先在应用内完成深圳大学统一认证，然后点击刷新。"}
          </Banner.Description>
        </Banner>
      )}
      {qwState === "unavailable" && (
        <Banner status="warning">
          <Banner.Title>SZU 考勤服务暂时不可用</Banner.Title>
          <Banner.Description>{qwMessage || "网络异常，稍后可点击刷新重试。"}</Banner.Description>
        </Banner>
      )}
      <div className="flex items-center justify-between gap-4">
        <h2 className="text-lg font-medium">今天的课次</h2>
        <Button variant="tonal" disabled={refreshing || loading} onClick={() => void load(true)}>
          <LuRefreshCw className={refreshing ? "animate-spin" : undefined} aria-hidden="true" />
          刷新
        </Button>
      </div>

      {loading ? (
        <div className="flex justify-center py-10">
          <LoadingIndicator />
        </div>
      ) : sessions.length === 0 ? (
        <Card variant="outlined" className="p-4 text-sm text-muted">
          今天没有课次。课表来自 SZU 考勤系统的缓存（6 小时刷新一次）。
        </Card>
      ) : (
        sessions.map((session, index) => (
          <Card
            key={`${session.course_name}-${session.start_ms}-${index}`}
            variant="outlined"
            className="p-4 flex items-center justify-between gap-3 cursor-pointer"
            onClick={() => setActive(session)}
          >
            <div className="flex flex-col gap-1">
              <span className="font-medium">{session.course_name}</span>
              <span className="text-xs text-muted">
                {session.building_name ?? "地点未知"}
                {session.classroom ? ` · ${session.classroom}` : ""}
              </span>
            </div>
            <span className="text-xs text-muted whitespace-nowrap">
              {formatSessionTime(session.start_ms, session.end_ms)}
            </span>
          </Card>
        ))
      )}

      {active && (
        <SzuSigninModal session={active} buildings={buildings} onClose={() => setActive(null)} />
      )}
    </section>
  );
}

function SzuSigninModal({
  session,
  buildings,
  onClose,
}: {
  session: TimetableSession;
  buildings: Building[];
  onClose: () => void;
}) {
  const [initial, setInitial] = useState<ResolvedPoint | null>(null);
  const [photoDataUri, setPhotoDataUri] = useState<string | null>(null);
  const [photoB64, setPhotoB64] = useState<string | null>(null);
  const [lastPhoto, setLastPhoto] = useState<LastPhotoView | null>(null);
  const [android, setAndroid] = useState(false);

  useEffect(() => {
    void isAndroidPlatform().then(setAndroid);
    let active = true;
    void tauriCore
      .invoke<LastPhotoView>("signin_get_last_photo", { courseName: session.course_name })
      .then((photo) => {
        if (active) setLastPhoto(photo);
      })
      .catch(() => undefined);
    void tauriCore
      .invoke<ResolvedPoint>("signin_resolve_radar_point", {
        courseName: session.course_name,
        courseTitle: session.course_name,
      })
      .then((point) => {
        if (active) setInitial(point);
      })
      .catch(() => undefined);
    return () => {
      active = false;
    };
  }, [session.course_name]);

  const handlePhotoPicked = (files: FileList | null) => {
    const file = files?.[0];
    if (!file) return;
    const reader = new FileReader();
    reader.onload = () => {
      const dataUri = String(reader.result ?? "");
      const comma = dataUri.indexOf(",");
      if (comma > 0) {
        setPhotoDataUri(dataUri);
        setPhotoB64(dataUri.slice(comma + 1));
        toast.info("已选择照片，提交时将自动压缩为 JPEG");
      }
    };
    reader.readAsDataURL(file);
  };

  const submit = async (selection: PointSelection) => {
    try {
      const args: Record<string, unknown> = { courseName: session.course_name };
      if (selection.kind === "building") {
        args.buildingId = selection.buildingId;
      } else if (selection.kind === "custom") {
        args.customLatitude = selection.latitude;
        args.customLongitude = selection.longitude;
        args.customIsWgs84 = selection.isWgs84;
      }
      if (photoB64) args.photoB64 = photoB64;
      await tauriCore.invoke("signin_submit_szu", args);
      toast.success("SZU 签到提交成功");
      onClose();
    } catch (error) {
      toast.danger("SZU 签到失败", { description: String(error) });
    }
  };

  const captureRef = useRef<HTMLInputElement | null>(null);
  const uploadRef = useRef<HTMLInputElement | null>(null);

  const preview = photoDataUri ?? (lastPhoto?.hasPhoto ? lastPhoto.thumbnail : null);

  return (
    <Dialog open headline={session.course_name} onClose={onClose}>
      <div className="flex flex-col gap-4">
        <RadarForm buildings={buildings} initial={initial} onSubmit={submit} />

        <div className="flex flex-col gap-2">
          <div className="flex items-center gap-2">
            <input
              ref={captureRef}
              type="file"
              accept="image/*"
              capture={android ? "environment" : undefined}
              aria-label="拍摄签到照片"
              className="hidden"
              data-testid="szu-photo-input"
              onChange={(event) => handlePhotoPicked(event.target.files)}
            />
            <Button variant="tonal" onClick={() => captureRef.current?.click()}>
              <LuCamera aria-hidden="true" />
              拍照
            </Button>
            <input
              ref={uploadRef}
              type="file"
              accept="image/*"
              aria-label="选择签到照片"
              className="hidden"
              data-testid="szu-photo-file-input"
              onChange={(event) => handlePhotoPicked(event.target.files)}
            />
            <Button variant="tonal" onClick={() => uploadRef.current?.click()}>
              <LuImageUp aria-hidden="true" />
              上传照片
            </Button>
          </div>
          {preview && (
            <img
              src={preview}
              alt="签到照片预览"
              className="max-h-40 rounded-lg object-cover w-fit"
            />
          )}
          <p className="text-xs text-muted">
            {photoB64
              ? "将使用本次选择的照片提交。"
              : lastPhoto?.hasPhoto
                ? "将使用该课程上一次的签到照片提交；也可以重新拍照/上传。"
                : "尚未上传过照片；SZU 签到需要照片时请先拍摄或选择。"}
          </p>
        </div>
      </div>
      <div slot="actions">
        <Button variant="text" onClick={onClose}>
          关闭
        </Button>
      </div>
    </Dialog>
  );
}
