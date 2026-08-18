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

export interface AgentSourceDetectionDto {
  provider: string;
  instructionPaths: string[];
  skillPaths: string[];
}

export interface GlobalAgentContextScanDto {
  sources: AgentSourceDetectionDto[];
  detectedInstructionSources: string[];
  detectedSkillSources: string[];
}

export async function scanAgentContext(id: string): Promise<AgentContextSnapshotDto> {
  return invoke<AgentContextSnapshotDto>("scan_agent_context", { id });
}

export async function scanGlobalAgentContext(): Promise<GlobalAgentContextScanDto> {
  return invoke<GlobalAgentContextScanDto>("scan_global_agent_context");
}
