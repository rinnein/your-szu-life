import type { EvaluationOverview, RunOutcome } from "../../evaluation/model";

const choices = (labels: [string, string, string, string]) => [
  { label: labels[0], valueLow: "90", valueHigh: "100", valueNormal: "92" },
  { label: labels[1], valueLow: "60", valueHigh: "90", valueNormal: "65" },
  { label: labels[2], valueLow: "30", valueHigh: "60", valueNormal: "30" },
  { label: labels[3], valueLow: "0", valueHigh: "30", valueNormal: "10" },
];

export const mockEvaluationOverview: EvaluationOverview = {
  fetchedAt: Date.now(),
  term: { id: 47, name: "26-27学年第1学期" },
  comment: "无",
  pendingTargets: 1,
  pendingRecords: 2,
  expiredRecords: 0,
  targets: [
    {
      id: "demo-36229-24510-47-1187",
      targetId: 1187,
      targetName: "常规课学生过程性评价指标-3",
      termName: "26-27学年第1学期",
      expiredDate: "2099-01-01",
      status: "pending",
      message: null,
      scoreType: 4,
      commentRequired: 2,
      staySeconds: 0,
      records: [
        {
          scId: 2242443,
          courseName: "离散数学",
          courseNo: "1500210001",
          employeeName: "演示教师甲",
          className: "202620271150021000101",
          currentTimes: 0,
          limitedTimes: 1,
          expiredDate: "2099-01-01",
          participate: true,
        },
        {
          scId: 2242444,
          courseName: "离散数学",
          courseNo: "1500210001",
          employeeName: "演示教师乙",
          className: "202620271150021000101",
          currentTimes: 0,
          limitedTimes: 1,
          expiredDate: "2099-01-01",
          participate: true,
        },
      ],
      questions: [
        {
          subId: 1237,
          subName: "教师是否教态大方得体，精神饱满，课堂投入度高？",
          kind: "options",
          choices: choices(["非常优秀", "良好", "一般", "待改进"]),
        },
        {
          subId: 1246,
          subName: "总体满意度",
          kind: "options",
          choices: choices(["非常满意", "比较满意", "一般", "不太满意"]),
        },
      ],
    },
    {
      id: "demo-36229-24508-47-1187",
      targetId: 1187,
      targetName: "常规课学生过程性评价指标-3",
      termName: "26-27学年第1学期",
      expiredDate: "2099-01-01",
      status: "done",
      message: null,
      scoreType: 4,
      commentRequired: 2,
      staySeconds: 0,
      records: [],
      questions: [],
    },
  ],
};

export const mockEvaluationRunOutcome: RunOutcome = {
  dryRun: false,
  summary: {
    submittedTargets: 1,
    submittedRecords: 2,
    failures: [],
  },
  plans: [],
};
