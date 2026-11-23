import { invoke } from "@tauri-apps/api/core";

export interface InstructionDocumentDto {
  id: string;
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
  globallyDisabledInstructionIds?: string[];
  globallyDisabledSkillIds?: string[];
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
  instructions: InstructionDocumentDto[];
  skills: SkillDescriptorDto[];
}

export async function scanAgentContext(id: string): Promise<AgentContextSnapshotDto> {
  return invoke<AgentContextSnapshotDto>("scan_agent_context", { id });
}

export async function scanGlobalAgentContext(options?: {
  customInstructionPaths?: string;
  customSkillPaths?: string;
}): Promise<GlobalAgentContextScanDto> {
  if (!options) {
    return invoke<GlobalAgentContextScanDto>("scan_global_agent_context");
  }
  return invoke<GlobalAgentContextScanDto>("scan_global_agent_context", {
    customInstructionPaths: options.customInstructionPaths,
    customSkillPaths: options.customSkillPaths,
  });
}
