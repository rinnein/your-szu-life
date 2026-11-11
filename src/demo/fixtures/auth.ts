import type { AuthState, Profile, UserData } from "../../auth";

/** 演示账户：纯字面量假数据，不对应任何真实学号。 */
export const mockProfile: Profile = {
  studentId: "20250001",
  campusCardNumber: "988801234567",
  hasPassword: true,
  name: "演示同学",
};

export const mockUserData: UserData = {
  studentId: mockProfile.studentId,
  name: mockProfile.name,
};

export const mockAuthState: AuthState = {
  Authenticated: { name: mockProfile.name, studentId: mockProfile.studentId },
};
