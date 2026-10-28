import { toast } from "./ui";
import { FeatureGrid, type Feature } from "./components";
import { PageSurface } from "./shell/PageSurface";
import { TopAppBar } from "./shell/TopAppBar";
import { AuthGate } from "./shell/AuthGate";
import {
  FaCalendarDays,
  FaChartLine,
  FaCirclePlay,
  FaClipboardList,
  FaGraduationCap,
} from "react-icons/fa6";

const STUDY_FEATURES: readonly Feature[] = [
  {
    label: "上课时间表",
    icon: FaCalendarDays,
    description: "查看各节次上课时间",
    to: "/study/class-timetable",
  },
  {
    label: "计软合集",
    icon: FaGraduationCap,
    description: "OJ等计软相关功能",
    to: "/study/computing",
  },
  {
    label: "UOOC刷课",
    icon: FaCirclePlay,
    description: "优课在线视频任务",
    to: "/study/uooc",
  },
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
  { label: "成绩查询", icon: FaChartLine, description: "暂未实现 · 即将支持" },
];

export default function StudyPage() {
  return (
    <AuthGate>
      <PageSurface maxWidth="2xl">
        <TopAppBar title="学习" description="课表、选课与计软相关功能" showBack={false} />

        <FeatureGrid
          features={STUDY_FEATURES}
          onPlaceholder={(label) => {
            toast.info(`${label}功能即将支持`, { description: "当前仅提供入口占位" });
          }}
        />
      </PageSurface>
    </AuthGate>
  );
}
