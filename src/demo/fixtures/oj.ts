import type {
  Contest,
  Draft,
  Group,
  Paper,
  Problem,
  Records,
  Snapshot,
  Submission,
} from "../../oj/types";
import { problemKey } from "../../oj/types";
import { mockProfile } from "./auth";

const site = (id: string, name: string, url: string): Snapshot["sites"][number] => ({
  id,
  name,
  url,
  builtin: true,
  reminders: true,
  enabled: true,
  username: mockProfile.studentId,
  sessionGeneration: 1,
  session: "ready",
  message: "",
  lastSync: Date.now() - 120_000,
  hasCache: true,
});

export const mockOjSnapshot: Snapshot = {
  sites: [site("npuoj", "校 OJ", "https://oj.example.edu.cn")],
  notices: [],
  companionPort: 0,
  desktop: true,
  companionEnabled: false,
  companionStatus: { connected: false, active: false, message: "" },
};

export const mockOjGroups: Group[] = [
  { id: "g-1", name: "2026 级程序设计基础" },
  { id: "g-2", name: "算法竞赛训练营" },
];

export const mockOjContests: Contest[] = [
  {
    id: "ct-1",
    groupId: "g-1",
    title: "第七周周赛",
    description: "模拟演示用比赛",
    begin: Date.now() - 3 * 86_400_000,
    end: Date.now() - 2 * 86_400_000,
    now: Date.now(),
    isExam: false,
  },
  {
    id: "ct-2",
    groupId: "g-1",
    title: "期中上机考试",
    description: "模拟演示用考试",
    begin: Date.now() + 2 * 86_400_000,
    end: Date.now() + 2 * 86_400_000 + 2 * 3_600_000,
    now: Date.now(),
    isExam: true,
  },
];

const submission = (id: number, problem: number, status: number, language: string): Submission => ({
  id,
  problem,
  status,
  language,
  create_time: new Date(Date.now() - id * 3_600_000).toISOString().replace("T", " "),
  time: 120 + id * 13,
  memory: 1024 * (128 + id * 8),
});

export const mockOjRecords: Records = {
  records: [
    submission(1013, 1002, 0, "C++"),
    submission(1012, 1001, 0, "C++"),
    submission(1011, 1003, 5, "Python"),
    submission(1010, 1001, 4, "Java"),
    submission(1009, 1004, 6, "C"),
  ],
  uncertain: [],
};

export const mockOjProgress = { accepted: 118, total: 240 };

const cppTemplate = `#include <bits/stdc++.h>
using namespace std;

int main() {
    ios::sync_with_stdio(false);
    cin.tie(nullptr);

    // TODO: 在这里写你的代码

    return 0;
}
`;

const problem = (
  id: number,
  title: string,
  description: string,
  input: string,
  output: string,
  samples: { input: string; output: string }[],
): Problem => ({
  id: String(id),
  displayLabel: String.fromCharCode(65 + ((id - 1001) % 26)),
  kind: 0,
  title,
  description,
  input,
  output,
  hint: "",
  timeLimit: 1000,
  memoryLimit: 256,
  samples,
  choices: [],
  multiple: false,
  blanks: 0,
  languages: ["cpp", "c", "java", "py"],
  templates: { cpp: cppTemplate },
  score: 100,
  ioMode: "standard",
});

/** 草稿按题目分别存放，语言固定成模板里有的 C++。 */
const draft = (code: string): Draft => ({
  code,
  language: "cpp",
  answer: "",
  options: null,
  revision: 1,
});

/** 演示草稿：一道已经写好的「两数之和」，让答题页打开就有代码可看。 */
const solutionSource = `#include <bits/stdc++.h>
using namespace std;

int main() {
    ios::sync_with_stdio(false);
    cin.tie(nullptr);

    int n, target;
    cin >> n >> target;
    vector<int> a(n);
    for (int &value : a) cin >> value;

    unordered_map<int, int> seen;
    for (int i = 0; i < n; ++i) {
        auto it = seen.find(target - a[i]);
        if (it != seen.end()) {
            cout << it->second << ' ' << i << '\\n';
            return 0;
        }
        seen.emplace(a[i], i);
    }
    return 0;
}
`;

const problems: Problem[] = [
  problem(
    1001,
    "两数之和",
    "<p>给定一个整数数组与一个目标值，请找出数组中<b>和为目标值</b>的两个下标。</p><p>保证每组输入有且仅有一组答案，且同一个元素不能重复使用。</p>",
    "<p>第一行两个整数 <code>n</code> 与 <code>target</code>；第二行是 <code>n</code> 个整数。</p>",
    "<p>输出两个下标（从 <code>0</code> 开始），用空格分隔。</p>",
    [
      { input: "4 9\n2 7 11 15\n", output: "0 1\n" },
      { input: "3 6\n3 2 4\n", output: "1 2\n" },
    ],
  ),
  problem(
    1002,
    "反转链表",
    "<p>给定一个单链表的头节点，请把链表反转，并输出反转后的序列。</p><p>输入用一个以 <code>-1</code> 结尾的整数序列表示链表，请按反转顺序输出（不含结尾的 <code>-1</code>）。</p>",
    "<p>一行整数，以 <code>-1</code> 表示链表结束。</p>",
    "<p>反转后的链表序列，空格分隔。</p>",
    [
      { input: "1 2 3 4 5 -1\n", output: "5 4 3 2 1\n" },
      { input: "-1\n", output: "\n" },
    ],
  ),
];

/** 演示用题面：让 OJ 答题页在无 IPC 的环境里也有完整内容可看。 */
export const mockOjPaper: Paper = {
  contest: mockOjContests[0],
  problems,
  drafts: {
    // 键必须是 `problemKey` 的 `kind:id` 形式，答题页按它取草稿。
    [`${problemKey(problems[0])}`]: draft(solutionSource),
  },
  account: mockProfile.studentId,
  epoch: 1,
  writable: true,
  submitted: null,
  sourceUrl: mockOjSnapshot.sites[0]?.url ?? "",
  preferences: {
    language: "cpp",
    optionsByLanguage: {},
    revision: 1,
  },
};
