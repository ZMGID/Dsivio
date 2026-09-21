export type TaskOrganization = { archived: boolean; pinned: boolean }
export type TaskOrganizations = Record<string, TaskOrganization>
export type TaskOrganizationPatch = Partial<TaskOrganization>
