import { Fragment } from "react";
import { Divider } from "material/react";
import { FeatureGroup, type Feature } from "./components";
import { PageSurface } from "./shell/PageSurface";
import { TopAppBar } from "./shell/TopAppBar";
import { AuthGate } from "./shell/AuthGate";
import {
  FaBookOpen,
  FaCalendarDays,
  FaChartLine,
  FaCirclePlay,
  FaClipboardCheck,
  FaClipboardList,
  FaCode,
  FaGraduationCap,
  FaMapLocationDot,
  FaVideo,
} from "react-icons/fa6";

/**
 * 学习页入口按使用频率分两组：
 *
 * - 日常：只要有课就会反复打开，组内也按打开频次从高到低排。
 * - 周期：只在学期里的特定时点（选课期、出成绩、评教窗口）才用得到。
 *
 * 分组之间插一条 material 的 `Divider`，由下方渲染逻辑按顺序生成。
 */
const STUDY_GROUPS: readonly { label: string; features: readonly Feature[] }[] = [
  {
    label: "日常",
    features: [
      {
        label: "课表",
        icon: FaCalendarDays,
        description: "本周课程、学期课表与待签提醒",
        to: "/study/timetable",
      },
      {
        label: "签到/打卡",
        icon: FaMapLocationDot,
        description: "畅课与 SZU 考勤签到",
        to: "/study/signin",
      },
      {
        label: "畅课 lms",
        icon: FaBookOpen,
        description: "课程待办与站内通知",
        to: "/study/lms",
      },
      {
        label: "UOOC刷课",
        icon: FaCirclePlay,
        description: "优课在线视频任务",
        to: "/study/uooc",
      },
      {
        label: "OJ",
        icon: FaCode,
        description: "班级、测验与编程练习",
        to: "/study/oj",
      },
      {
        label: "监察巡课",
        icon: FaVideo,
        description: "课程直播与录播回放",
        to: "/study/patrol",
      },
    ],
  },
  {
    label: "周期",
    features: [
      {
        label: "选课",
        icon: FaClipboardList,
        description: "查课、抢课与选课期间课表",
        to: "/study/enrollment",
      },
      {
        label: "学业完成查询",
        icon: FaGraduationCap,
        description: "培养方案、学分与课程完成情况",
        to: "/study/academic",
      },
      {
        label: "成绩查询",
        icon: FaChartLine,
        description: "各学期课程成绩、学分与 GPA",
        to: "/study/grades",
      },
      {
        label: "教学评价",
        icon: FaClipboardCheck,
        description: "学生评教、待评清单与一键完成",
        to: "/study/evaluation",
      },
    ],
  },
];

export default function StudyPage() {
  return (
    <AuthGate>
      <PageSurface maxWidth="2xl">
        <TopAppBar title="学习" description="课表、选课与编程练习等功能" showBack={false} />

        {STUDY_GROUPS.map((group, index) => (
          <Fragment key={group.label}>
            {index > 0 && <Divider className="feature-groups-divider" />}
            <FeatureGroup label={group.label} features={group.features} />
          </Fragment>
        ))}
      </PageSurface>
    </AuthGate>
  );
}
