// In-memory test backend only. This entry never ships in the production bundle.
const initialWorkspaces=[{id:'alpha',name:'开发项目 Alpha',path:'/fixture/projects/alpha',runtime:{tool_profile:'compact',permission_mode:'safe',inherit_global_execution_policy:true,history_recording:true,history_context_sessions:[]}},{id:'beta',name:'很长的项目名称 Beta 与持续迭代的工作区',path:'/fixture/projects/a-very-long-path/beta',runtime:{tool_profile:'compact',permission_mode:'safe',inherit_global_execution_policy:true,history_recording:true,history_context_sessions:[]}}];
const goal={id:'goal',title:'统一开发过程与验收',objective:'让用户可以清楚了解当前任务与证据。',status:'active',success_criteria:[{id:'c',text:'变更与验证记录可追溯',completed:false}],constraints:[],plan_ids:['review'],review_summary:null};
const plan={id:'review',goal_id:'goal',title:'统一页面布局与任务验收',objective:'完成导航、任务详情、变更和设置的统一。',status:'awaiting_acceptance',steps:[{id:'done',title:'完成导航外壳',status:'completed',notes:'保留所有现有能力。'},{id:'skipped',title:'无需数据迁移',status:'skipped',notes:null},{id:'current',title:'人工复核界面与交互',status:'pending',notes:'阅读变更及验证记录。'}],review_summary:'导航与核心页面已调整；请结合变更和记录验收。',review_feedback:null};
const baseState={schema_version:1,revision:1,mode:'plan',focus_goal_id:'goal',focus_plan_id:'review',goals:[goal],plans:[plan],execution:{goal_id:'goal',plan_id:'review',step_id:'current',task_id:null,state:'idle',last_error:null,changed_files:['src/router.ts'],verification:['类型检查完成；截图与人工交互需要继续复核。'],updated_at:'2026-10-07T00:00:00Z'}};
let workspaces=structuredClone(initialWorkspaces);let states={alpha:structuredClone(baseState),beta:{...structuredClone(baseState),focus_plan_id:null,focus_goal_id:null,plans:[],goals:[],execution:{...baseState.execution,plan_id:null,goal_id:null,state:'blocked',last_error:'测试环境：连接超时，请查看诊断。',verification:[]}}};
let runtime={authType:'bearer',permissionMode:'safe',allowedCommands:'git,cargo',executablePaths:'',aiInstructions:'',instructionSources:[],skillSources:[],customInstructionPaths:'',customSkillPaths:'',allowLanAccess:false,restoreRuntimeStateOnLaunch:false,migrationNotice:''};
let proxy={mode:'none',url:''};let gateway={enabled:false,localPort:28765,tunnelType:'none',publicUrl:'',cloudflareMode:'quick',frpProfileId:'',frpServer:'',frpSubdomain:'',frpServerPort:7000,useProxy:true};let download={githubMirror:'',proxyMode:'system',proxyUrl:''};let failSave=false,failReview=false,armPostReviewFailure=false,failPlanningRead=false;let calls=[];let callback=0;
const history={context_revision:'fixture',history_count:1,sessions:[{number:1,path:'/fixture/history/1.md',title:'开发页面布局',session_key:'fixture-session',created_at:'2026-10-07T00:00:00Z',updated_at:'2026-10-07T01:00:00Z',bytes:100,entry_count:2,latest_focus:'完成导航并准备验收',key_files:['src/router.ts'],snippets:[{turn_id:'1',timestamp:'2026-10-07T01:00:00Z',text:'已完成核心导航，保留旧设置地址兼容。'}]}]};
const reviews=[{id:'snapshot',scope:'operation',createdAt:1791331200,summary:'导航修改的冻结快照',files:1,additions:10,deletions:3,sessionId:'fixture-session'}];
const output=document.getElementById('fixture-result');
document.getElementById('fail-save').onclick=()=>{failSave=true;output.textContent='下一次保存将失败';};document.getElementById('fail-review').onclick=()=>{failReview=true;output.textContent='下一次验收请求将失败';};document.getElementById('post-review-fail').onclick=()=>{armPostReviewFailure=true;output.textContent='下一次验收成功后状态刷新将失败';};document.getElementById('empty').onclick=()=>{sessionStorage.setItem('fixture-empty','1');location.reload();};document.getElementById('reset').onclick=()=>{sessionStorage.removeItem('fixture-empty');location.reload();};if(sessionStorage.getItem('fixture-empty'))workspaces=[];
const overview=()=>({state:'stopped',localEndpoint:'http://127.0.0.1:28765/mcp',publicEndpoint:'',workspaceCount:workspaces.length,sessionCount:0,registryRevision:1,sessions:[]});
async function invoke(cmd,args={}){
 calls.push({cmd,args});
 if(cmd.startsWith('plugin:'))return cmd.includes('is_minimized')?false:1;
 if((cmd.startsWith('set_')||cmd.startsWith('save_')||cmd==='update_workspace')&&failSave){failSave=false;throw new Error('测试保存失败：草稿应保留。');}
 if(cmd==='list_workspaces')return structuredClone(workspaces);
 if(['restore_runtime_state','set_last_workspace'].includes(cmd))return;
 if(cmd==='get_last_workspace_id')return 'alpha';
 if(['get_runtime_status','get_global_mcp_overview'].includes(cmd))return overview();
 if(cmd==='get_planning_state'){if(failPlanningRead){failPlanningRead=false;throw new Error('测试：验收已保存，随后读取失败。');}return structuredClone(states[args.workspaceId]);}
 if(['accept_plan_review','reject_plan_review','accept_goal_review','reject_goal_review'].includes(cmd)){
  if(failReview){failReview=false;throw new Error('测试验收请求失败：可以重试，输入应保留。');}
  if(armPostReviewFailure){armPostReviewFailure=false;failPlanningRead=true;}
  const state=states[args.workspaceId];const item=cmd.includes('plan')?state.plans.find(p=>p.id===args.planId):state.goals.find(g=>g.id===args.goalId);item.status=cmd.startsWith('accept')?'completed':'active';item.review_feedback=args.feedback??null;output.textContent=cmd.startsWith('accept')?'验收状态：已完成':'已收到修改意见：'+args.feedback;return structuredClone(item);
 }
 if(cmd==='set_planning_mode'){states[args.workspaceId].mode=args.mode;return structuredClone(states[args.workspaceId]);}
 if(cmd==='get_workspace_git_summary')return {available:true,branch:'main',changedFiles:3,ahead:0,behind:0,lastCommit:'fixture: unify navigation',subRepositories:[]};
 if(['create_workspace_review_url','create_session_review_url'].includes(cmd)){const scope=cmd.includes('session')?'session':'workspace';const review={id:scope+'-snapshot',scope,createdAt:1791331200,summary:scope==='session'?'会话范围冻结快照':'工作区范围冻结快照',files:1,additions:10,deletions:3,sessionId:scope==='session'?args.sessionId:null};if(!reviews.some(item=>item.id===review.id))reviews.push(review);return {reviewId:review.id,url:'http://fixture.invalid/review/'+review.id};}
 if(cmd==='issue_workspace_review_url')return 'http://fixture.invalid/review/'+args.reviewId;
 if(cmd==='open_url'){output.textContent='打开冻结快照：'+args.url;return;}
 if(cmd==='detect_installed_ides')return [];
 if(cmd==='list_workspace_reviews')return structuredClone(reviews);
 if(cmd==='list_history_sessions')return history;
 if(cmd==='get_service_usage_stats')return [{service:'mcp',workspaceId:args.id,estimatedTokens:1200,estimatedInputTokens:800,estimatedOutputTokens:400,estimatedToolCallTokens:600,requestCount:12,toolCallCount:8,errorCount:1,inputBytes:3200,outputBytes:1600}];
 if(cmd==='run_global_health_checks')return [{label:'本地 MCP',ok:false,detail:'测试环境未启动服务',hint:'连接页查看'}];
 if(cmd==='check_global_gateway_health')return [{label:'本地端点',ok:false,detail:'服务已停止，连接未检查。'}];
 if(cmd==='get_global_gateway_config')return structuredClone(gateway);
 if(cmd==='set_global_gateway_config'){gateway=structuredClone(args.config);return;}
 if(cmd==='get_global_runtime_settings')return structuredClone(runtime);
 if(cmd==='set_global_runtime_settings'){runtime=structuredClone(args.runtime);return;}
 if(cmd==='get_proxy')return structuredClone(proxy);
 if(cmd==='set_proxy'){proxy=structuredClone(args.proxy);return;}
 if(cmd==='list_frp_profiles')return [];
 if(cmd==='get_shared_secret')return 'fixture-secret';
 if(cmd==='get_download_config')return structuredClone(download);
 if(cmd==='set_download_config'){download=structuredClone(args.config);return;}
 if(cmd==='list_software')return [];
 if(cmd==='get_workspace_activity_metrics')return {events:[],totalOperations:2,totalFiles:3,totalAdditions:10,totalDeletions:3};
 if(cmd==='list_tool_audit_records')return {records:[{timestampMs:1791331200000,durationMs:180,requestId:'fixture-request',sessionId:'fixture-session',workspaceId:'alpha',toolName:'apply_patch',wrapperToolName:null,authMethod:'shared_bearer',clientId:'fixture',outcome:'success',errorCategory:null,requestBytes:100,responseBytes:80,changeId:'snapshot',changedFileCount:1}],total:1,errorCount:0,workspaceCount:1,offset:0,limit:50,retentionDays:30,healthMessage:null};
 if(cmd==='scan_global_agent_context')return {sources:[],detectedInstructionSources:[],detectedSkillSources:[]};
 if(cmd==='scan_agent_context')return {instructions:[],skills:[],detectedInstructionSources:[],detectedSkillSources:[]};
 if(cmd==='get_webview_memory_sample')return {supported:false,webviewMb:0};
 if(cmd==='read_workspace_logs')return [{name:'fixture.log',content:'测试环境连接超时。此日志只用于验证界面。'}];
 if(cmd==='update_workspace'){const next=args.profile??args.workspace;workspaces=workspaces.map(w=>w.id===next.id?next:w);return;}
 if(cmd==='check_app_update')return {currentVersion:'0.3.4',latestVersion:'0.3.4',updateAvailable:false};
 throw new Error('未配置的测试接口：'+cmd);
}
window.__TAURI_EVENT_PLUGIN_INTERNALS__={unregisterListener:()=>{}};
window.__TAURI_INTERNALS__={invoke,metadata:{currentWindow:{label:'main'},currentWebview:{label:'main'}},transformCallback:()=>++callback,unregisterCallback:()=>{},convertFileSrc:path=>path};
await import('/src/main.ts');
