import { invoke } from "@tauri-apps/api/core";

export interface InstructionDocumentDto {
  provider: string;
  path: string;
  scope: string;
  content: string;
}

export interface SkillDescriptorDto {
  id: string;
  name: string;
  description: string;
  provider: string;
  path: string;
  scope: string;
}

export interface AgentContextSnapshotDto {
  instructions: InstructionDocumentDto[];
  skills: SkillDescriptorDto[];
  renderedInstructions: string;
}

export async function scanAgentContext(id: string): Promise<AgentContextSnapshotDto> {
  return invoke<AgentContextSnapshotDto>("scan_agent_context", { id });
}
