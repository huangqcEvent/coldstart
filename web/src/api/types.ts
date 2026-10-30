export interface ProjectStatus {
  projectId: string;
  name: string;
  rootPath: string;
  createdAt: string;
  eventCount: number;
}

export interface CommandError {
  code: string;
  message: string;
}
