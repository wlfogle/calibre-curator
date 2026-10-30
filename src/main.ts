import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { open, save } from "@tauri-apps/plugin-dialog";
import "./style.css";

type Series = { name: string | null; index: number | null };
type Hierarchy = [Series, Series, Series, Series];
type Citation={segment:number;quote:string};
type Provider={name:string;version:string;identify:boolean;enabled:boolean};
type MetadataSettings={fetch_binary:string;search_all:boolean;providers:string[];timeout_seconds:number;cache_days:number};
type LookupResult={id:string;library:string;book_id:number;book_uuid:string;provider:Provider;fetched_at:number;cached:boolean;
  match_status:string;note:string;identity_confirmed:boolean;source_id:string|null;
  metadata:{title:string;authors:string[];identifiers:Record<string,string>;publisher:string|null;description:string;url:string|null}|null};
type SynopsisDraft={id:string;lookup_id:string;library:string;book_id:number;book_uuid:string;title:string;source_id:string;source_hash:string;
  text:string;notes:string;citations:Citation[];model:string;digest:string;prompt_version:string;generated_at:number;cached:boolean;
  prompt_tokens:number;output_tokens:number;approved:boolean;state:string;operation_id:string|null};
type AiSettings={enabled:boolean;endpoint:string;model:string;allow_lan:boolean;timeout_seconds:number};
type AiCandidate={title:string;authors:string[];edition:string;notes:string;citations:Citation[];
  fields:{slot:number;name:string;index:number|null;citations:Citation[]}[]};
type AiRun={id:string;source_id:string;source_hash:string;model:string;digest:string;prompt_version:string;
  generated_at:number;cached:boolean;identity_context_expansions:number;input_bytes:number;prompt_tokens:number;output_tokens:number;duration_ms:number;
  fingerprints:string[];extraction:{status:string;explanation:string;warnings:string[];candidates:AiCandidate[]}};
type Book = { id:number; uuid:string; title:string; authors:string[]; identifiers:Record<string,string>;
  hierarchy:Hierarchy; publisher:string|null; has_comments:boolean; formats:string[] };
type Reference = { id:string; title:string; authors:string[]; aliases:string[]; identifiers:Record<string,string>;
  book_uuids:string[]; hierarchy:Hierarchy; source_id:string; evidence:string; approved:boolean;
  origin:string; coverage:string; before:string[]; fixed:boolean; edition:string;field_mask?:boolean[]|null;
  ai_provenance?:{model:string;digest:string;source_hash:string;citations:Citation[];extracted_slots:number[]}|null };
type Source = { id:string; name:string; location:string; sha256:string; kind:string;
  segments:{ordinal:number;kind:string;text:string}[] };
type SourceReview={fingerprint:string;universe:string;authors:string[];aliases:Record<string,string>};
type DocumentPreview={entries:{reference:Reference;status:string;matches:number}[];skipped:number[]};
type ResearchRun={query:string;fetched_at:number;cached:boolean;status:string;notes:string[];leads:{url:string;title:string;engine:string}[]};
type Proposal = { book:Book; status:string; reason:string; reference_id:string|null;
  after:Hierarchy|null; changed:number[]; blocked:boolean; pinned:boolean[] };
type Operation = { id:string; created:number; library:string; phase:string; kind:string;metadata_field?:string;
  backup:string|null; error:string|null; changes:unknown[]; verification:unknown };
type Settings = { library:string; columns:[string,string,string]; calibre_binary:string };
type Alias = { id:string; existing_series:string; universe:string };
type Snapshot = { workspace:string; data:{settings:Settings; references:Reference[]; sources:Source[];aliases:Alias[];
  ai_settings:AiSettings;ai_models:{name:string;digest:string;size:number;usable:boolean;reason:string}[];
  ai_runs:AiRun[];ai_decisions:Record<string,{status:string;reference_id:string|null}>;
  metadata_settings:MetadataSettings;metadata_providers:Provider[];metadata_results:LookupResult[];synopsis_drafts:SynopsisDraft[];
  source_reviews:Record<string,SourceReview>;research_runs:ResearchRun[];repair_notes:string[]};
  proposals:Proposal[]; history:Operation[]; issues:Record<string,string> };

let state:Snapshot;
let tab="Repair", busy=false, query="", status="", page=0;
let universeFilter:string|null=null;
let aiSelection={source_id:"",start:1,end:1,universe:""};
let synopsisBook=0;
const synopsisSelected=new Set<string>();
const selected=new Map<number,Set<number>>();
const root=document.querySelector<HTMLDivElement>("#app")!;
const heading=document.createElement("header"), nav=document.createElement("nav"), content=document.createElement("main");
const notice=document.createElement("div"); notice.className="notice"; notice.setAttribute("role","status");
root.append(heading,nav,notice,content);
new ResizeObserver(()=>document.documentElement.style.setProperty("--nav-height",`${nav.offsetHeight}px`)).observe(nav);
function el<K extends keyof HTMLElementTagNameMap>(tag:K,text="",className=""):HTMLElementTagNameMap[K] {
  const node=document.createElement(tag); if(text)node.textContent=text;if(className)node.className=className;return node;
}
function button(text:string,fn:()=>void|Promise<unknown>,style=""):HTMLButtonElement {
  const b=el("button",text,style);b.disabled=busy;
  b.type="button";b.onclick=async()=>{try{await fn();}catch(error){message(String(error),true);}};return b;
}
function message(text:string,error=false) {notice.textContent=text;notice.className=`notice ${error?"error":""}`;}
function field(label:string,value="",type="text") {
  const wrapper=el("label"),input=el("input");wrapper.append(el("span",label));input.type=type;input.value=value;wrapper.append(input);
  return {wrapper,input};
}
function checkbox(label:string,checked:boolean,change:(checked:boolean)=>void) {
  const wrapper=el("label","","check"),input=el("input");input.type="checkbox";input.checked=checked;
  input.onchange=()=>change(input.checked);wrapper.append(input,document.createTextNode(label));return wrapper;
}
function series(value:Series):string {return value.name?`${value.name} · ${value.index??"?"}`:"—";}
function hierarchy(values:Hierarchy) {
  const node=el("div","","hierarchy");values.forEach((v,i)=>node.append(el("div",`${i+1}: ${series(v)}`)));return node;
}
async function confirmAction(title:string,explanation:string):Promise<boolean> {
  const dialog=el("dialog"),box=el("div","","modal");
  box.append(el("h2",title),el("p",explanation));
  const cancel=button("Cancel",()=>dialog.close("cancel")),yes=button("Confirm",()=>dialog.close("yes"),"primary");
  box.append(cancel,yes);dialog.append(box);document.body.append(dialog);dialog.showModal();
  return new Promise(resolve=>dialog.onclose=()=>{const yes=dialog.returnValue==="yes";dialog.remove();resolve(yes);});
}
async function act(action:string,payload:unknown={},success="Completed. No library changes unless you explicitly applied or reversed an operation.") {
  if(busy)return;busy=true;document.body.classList.add("busy");message(`Working: ${action}… Keep this window open.`);
  document.querySelectorAll<HTMLButtonElement>("button").forEach(b=>b.disabled=true);
  try {state=await invoke<Snapshot>("dispatch",{action,payload});selected.clear();render();message(success);}
  catch(error) {
    try{state=await invoke<Snapshot>("dispatch",{action:"snapshot",payload:{}});render();}catch{/* Keep the original error. */}
    message(String(error),true);throw error;
  }
  finally {busy=false;document.body.classList.remove("busy");document.querySelectorAll<HTMLButtonElement>("button").forEach(b=>b.disabled=false);}
}
function render() {
  window.scrollTo(0,0);
  heading.replaceChildren(el("div","Calibre Curator","brand"),
    el("div","Local rules. Source-backed metadata. No AI credits.","muted"));
  nav.replaceChildren(...["Repair","Sources","History","Advanced"].map(name=>
    button(name,()=>{tab=name;if(name==="Library")universeFilter=null;render();},tab===name?"active":"")));
  if(!["Repair","Sources","History"].includes(tab)) {
    const tools=el("select");
    for(const name of ["Advanced","Library","Universes","References & sources","AI assistant","Synopses","Review","Diagnostics","Settings"]) {
      const option=el("option",name);option.value=name;option.selected=tab===name;tools.append(option);
    }
    tools.onchange=()=>{tab=tools.value;render();};nav.append(tools);
  }
  content.replaceChildren();
  if(!state)return;
  if(tab==="Repair")repair();
  else if(tab==="Sources")sourceWorkspace();
  else if(tab==="Advanced") {
    title("Advanced tools","Normal repair uses the Repair and Sources screens. These tools expose individual rules, model settings and diagnostics.");
    for(const name of ["Library","Universes","References & sources","AI assistant","Synopses","Review","Diagnostics","Settings"])
      content.append(button(name,()=>{tab=name;render();}));
  }
  else if(tab==="Library")audit();
  else if(tab==="Universes")universes();
  else if(tab==="AI assistant")aiWorkspace();
  else if(tab==="Synopses")synopses();
  else if(tab==="References & sources")references();
  else if(tab==="Review")review();
  else if(tab==="History")history();
  else if(tab==="Diagnostics")diagnostics();
  else settings();
  requestAnimationFrame(()=>document.documentElement.style.setProperty("--nav-height",`${nav.offsetHeight}px`));
}
function title(text:string,description:string) {content.append(el("h1",text),el("p",description,"muted"));}
function sourceWorkspace() {
  title("Your reference sources","Add a file or website once. Verify its book order and series mappings once; future scans reuse them.");
  content.append(button("Add reference file",async()=>{
    const path=await open({multiple:false,filters:[{name:"References",extensions:["odt","html","htm","csv","json"]}]});
    if(typeof path==="string")await act("import",{location:path,universe:"",remote:false},"Source added. Verify its order and mappings below.");
  },"primary"),button("Add website",()=>importDialog("",true)));
  for(const source of state.data.sources.filter(s=>s.kind!=="calibre-metadata")) {
    const card=el("section","","card"),verified=state.data.source_reviews[source.id];
    card.append(el("h2",source.name),el("p",source.location),el("p",verified?`Verified for ${verified.universe}`:"Not yet verified"),
      button(verified?"Review source mappings":"Verify source",()=>verifySourceDialog(source)));
    if(verified)card.append(button("Revoke verification",async()=>{
      if(await confirmAction("Revoke this source?","Its document-generated corrections stop being eligible. Existing library metadata is not changed."))
        await act("source_revoke",{id:source.id});
    }));
    const details=el("details");details.append(el("summary","Source text and provenance"),el("small",`SHA-256: ${source.sha256}`),
      el("pre",source.segments.map(s=>`${s.ordinal}: ${s.text}`).join("\n")));card.append(details);content.append(card);
  }
  const discovery=el("details");discovery.append(el("summary","Find additional author or series websites (DuckDuckGo)"));
  const query=field("Author, series, or research query");
  discovery.append(query.wrapper,button("Find websites",async()=>{
    if(await confirmAction("Search DuckDuckGo?","Send only this query to DuckDuckGo. Returned links are unverified leads; a blocked search does not prevent local-source repairs."))
      await act("research",{query:query.input.value,refresh:false},"Discovery finished. Links are leads, not verified sources.");
  }));
  for(const run of state.data.research_runs) {
    const item=el("details");item.open=run===state.data.research_runs[0];
    item.append(el("summary",`${run.query} · ${run.status}${run.cached?" · cached":""}`),
      el("small",new Date(run.fetched_at*1000).toLocaleString()));
    for(const note of run.notes)item.append(el("p",note,"warning"));
    for(const lead of run.leads) {
      const row=el("section");row.append(el("strong",lead.title),el("p",`${lead.url} · ${lead.engine} · unverified lead`));
      if(lead.url.startsWith("https://"))row.append(button("Fetch as reference",()=>importDialog(lead.url,true)));
      else row.append(el("small","HTTP-only lead; supply a verified public HTTPS destination to fetch."));
      item.append(row);
    }
    discovery.append(item);
  }
  content.append(discovery);
}
function verifySourceDialog(source:Source) {
  const dialog=el("dialog"),box=el("div","","modal wide"),existing=state.data.source_reviews[source.id];
  const universe=field("Main series / universe",existing?.universe??""),
    authors=field("Author context (separate multiple authors with |)",existing?.authors.join(" | ")??"");
  const aliases=el("textarea");aliases.rows=3;
  aliases.value=Object.entries(existing?.aliases??{}).map(([a,b])=>`${a} = ${b}`).join("\n");
  const aliasLabel=el("label");aliasLabel.append(el("span","Optional subseries name aliases, one per line: source label = preferred name"),aliases);
  const report=el("div");let checked:SourceReview|null=null;
  const config=():SourceReview=>{
    const mapping:Record<string,string>={};
    for(const line of aliases.value.split("\n").filter(s=>s.trim())) {
      const at=line.indexOf("=");if(at<1||!line.slice(at+1).trim())throw new Error("Aliases need source label = preferred name.");
      mapping[line.slice(0,at).trim()]=line.slice(at+1).trim();
    }
    return {fingerprint:"",universe:universe.input.value.trim(),authors:authors.input.value.split("|").map(s=>s.trim()).filter(Boolean),aliases:mapping};
  };
  const verify=button("Verify this source and use its mappings",async()=>{
    if(!checked||JSON.stringify(checked)!==JSON.stringify(config()))throw new Error("Preview the current settings first.");
    await act("source_verify",{id:source.id,review:checked},"Source verified. Matching corrections are ready in Repair; no library writes.");
    dialog.close();
  },"primary");verify.disabled=true;
  for(const input of [universe.input,authors.input,aliases])input.oninput=()=>{checked=null;verify.disabled=true;};
  box.append(el("h2",`Verify ${source.name}`),universe.wrapper,authors.wrapper,aliasLabel,
    el("p","Main-series positions follow every listed book and novella, including books you do not own. Numbered subseries retain their independent order. Headings are not subseries; unsupported fields stay unchanged."),
    button("Preview mappings",async()=>{
      checked=null;verify.disabled=true;const value=config();
      const preview=await invoke<DocumentPreview>("preview_source",{id:source.id,review:value});
      report.replaceChildren(el("p",`${preview.entries.length} work entries; ${preview.skipped.length} non-entry segments excluded. Check the exclusions before verifying.`));
      const table=el("table"),head=el("tr");["Work","Main series","Subseries","Library match"].forEach(s=>head.append(el("th",s)));table.append(head);
      for(const entry of preview.entries) {
        const row=el("tr"),r=entry.reference;
        row.append(el("td",r.title),el("td",series(r.hierarchy[0])),el("td",r.hierarchy.slice(1).map(series).join(" / ")),el("td",entry.status));table.append(row);
      }
      report.append(table);
      const skipped=el("details");skipped.append(el("summary","Excluded source text"),
        el("pre",source.segments.filter(s=>preview.skipped.includes(s.ordinal)).map(s=>`${s.ordinal}: ${s.text}`).join("\n")));
      report.append(skipped);
      checked=value;verify.disabled=!preview.entries.length;
    }),report,verify,button("Cancel",()=>dialog.close()));
  dialog.append(box);document.body.append(dialog);dialog.onclose=()=>dialog.remove();dialog.showModal();
}
function repair() {
  title("Repair your library","Scan → use verified sources → preview and apply fixes. Existing metadata is preserved when filling missing fields.");
  content.append(el("p",state.data.settings.library),
    button("1. Scan library",()=>act("scan",{},"Scan complete. Saved verified sources have been matched; no library changes."),"primary"),
    button("Manage reference sources",()=>{tab="Sources";render();}),
    button("Library settings",()=>{tab="Settings";render();}));
  if(!state.proposals.length) {content.append(el("p","Scan your library to begin."));return;}
  const eligible=state.proposals.filter(p=>!p.blocked&&p.after&&p.changed.some(i=>!p.pinned[i]));
  const missing=state.proposals.map(p=>p.book).filter(b=>!b.authors.some(a=>normalized(a)==="calibre")&&
    !(b.formats.length&&b.formats.every(f=>f==="CSV"||f==="XML"))&&(!b.has_comments||!b.publisher?.trim()||!Object.keys(b.identifiers).length));
  content.append(el("p",`${state.proposals.length} books · ${eligible.length} supported series corrections · ${missing.length} books missing synopsis, publisher or identifiers`));
  const filter=field("Filter books (optional)"),area=el("div");
  content.append(filter.wrapper,button("2. Find missing metadata",async()=>{
    const needle=filter.input.value.toLowerCase();
    const books=missing.filter(b=>`${b.title} ${b.authors.join(" ")}`.toLowerCase().includes(needle)).slice(0,20);
    if(!books.length)throw new Error("No missing metadata in the current filter.");
    if(await confirmAction(`Look up ${books.length} books?`,
      "Share their titles, authors and recognized identifiers with all enabled Calibre identification providers (or your saved custom selection). No ebook files are uploaded. This can take several minutes per book. Nothing is written to your library."))
      await act("repair_prepare",books.map(b=>b.id),"Provider lookup finished. Review fills below; unavailable or mismatched evidence is held.");
  }),el("small","Online lookups run in batches of up to 20 books; local series matching covers the whole library."),area);
  for(const note of state.data.repair_notes)content.append(el("p",note,"warning"));
  const seriesPicks=new Set(eligible.map(p=>p.book.id));
  type Fill={lookup_id:string;book:Book;synopsis:boolean;publisher:boolean;identifiers:boolean};
  const fills=new Map<number,Fill>();
  const excludedFills=new Set<number>();
  function rows() {
    const needle=filter.input.value.toLowerCase();
    area.replaceChildren(el("h2","3. Preview fixes"));
    const seriesBox=el("details");seriesBox.open=true;seriesBox.append(el("summary",`Series / subseries corrections (${eligible.length})`));
    seriesBox.append(checkbox("Select all supported series corrections",seriesPicks.size===eligible.length,v=>{
      seriesPicks.clear();if(v)for(const p of eligible)seriesPicks.add(p.book.id);rows();
    }));
    for(const p of eligible.filter(p=>`${p.book.title} ${p.book.authors.join(" ")}`.toLowerCase().includes(needle))) {
      const card=el("details");card.append(el("summary",`${p.book.title} — ${p.book.authors.join(" & ")}`),
        checkbox("Include these unlocked series corrections",seriesPicks.has(p.book.id),v=>{if(v)seriesPicks.add(p.book.id);else seriesPicks.delete(p.book.id);}),
        el("p","Current"),hierarchy(p.book.hierarchy),el("p","Proposed (protected fields remain unchanged)"),
        hierarchy(p.after!.map((v,i)=>p.pinned[i]?p.book.hierarchy[i]:v) as Hierarchy),el("p",p.reason));
      seriesBox.append(card);
    }
    area.append(seriesBox,el("h2","Missing metadata fills"));
    for(const b of missing.filter(b=>`${b.title} ${b.authors.join(" ")}`.toLowerCase().includes(needle)).filter(b=>
      state.data.metadata_results.some(r=>r.library===state.data.settings.library&&r.book_uuid===b.uuid)).slice(0,20)) {
      const results=state.data.metadata_results.filter(r=>r.library===state.data.settings.library&&r.book_uuid===b.uuid);
      if(!results.length)continue;
      const card=el("section","","card"),choices=el("select"),preview=el("div");
      const none=el("option","Leave unchanged");none.value="";choices.append(none);
      for(const r of results) {
        const option=el("option",`${r.provider.name} · ${r.match_status}`);option.value=r.id;
        option.disabled=r.match_status!=="compatible"||!r.metadata;option.selected=fills.get(b.id)?.lookup_id===r.id;choices.append(option);
      }
      if(!fills.has(b.id)&&!excludedFills.has(b.id)) {
        const compatible=results.filter(r=>r.match_status==="compatible"&&r.metadata);
        const targets=new Set(compatible.map(r=>JSON.stringify([r.metadata!.publisher,r.metadata!.identifiers,r.metadata!.description])));
        if(targets.size===1&&compatible[0])choices.value=compatible[0].id;
      }
      const show=()=>{
        const prior=fills.get(b.id);
        fills.delete(b.id);preview.replaceChildren();const r=results.find(r=>r.id===choices.value),m=r?.metadata;
        if(!r||!m){excludedFills.add(b.id);return;}
        excludedFills.delete(b.id);
        const f:Fill=prior?.lookup_id===r.id?prior:{lookup_id:r.id,book:b,synopsis:true,publisher:true,identifiers:true};fills.set(b.id,f);
        preview.append(el("p",`${m.title} — ${m.authors.join(" & ")}`),el("p",r.note,"warning"));
        const descriptionBytes=new TextEncoder().encode(m.description).length;
        if(!b.has_comments&&descriptionBytes>=80&&descriptionBytes<=4000&&m.description.split(/\s+/).length<=220) {
          preview.append(checkbox("Fill missing synopsis",f.synopsis,v=>f.synopsis=v),el("p",m.description));
        } else {f.synopsis=false;if(!b.has_comments)preview.append(el("p","No short usable description. Synopsis stays unchanged; advanced tools can prepare a shortened draft."));}
        if(!b.publisher?.trim()&&m.publisher?.trim()&&new TextEncoder().encode(m.publisher.trim()).length<=500) {
          preview.append(checkbox(`Fill publisher: ${m.publisher}`,f.publisher,v=>f.publisher=v));
        } else f.publisher=false;
        const added=Object.fromEntries(Object.entries(m.identifiers).filter(([k])=>!b.identifiers[k]?.trim()));
        if(Object.keys(added).length) {
          preview.append(checkbox(`Fill missing identifiers: ${JSON.stringify(added)}`,f.identifiers,v=>f.identifiers=v));
        } else f.identifiers=false;
        preview.append(el("small","Including this result in Apply confirms that you reviewed the displayed work, edition and field values."));
      };
      choices.onchange=show;
      card.append(el("h3",b.title),choices,preview);
      const diagnostics=el("details");diagnostics.append(el("summary",`${results.length} provider outcomes`));
      for(const r of results)diagnostics.append(el("p",`${r.provider.name}: ${r.match_status} — ${r.note}`));
      card.append(diagnostics);area.append(card);
      if(choices.value)show();
    }
  }
  filter.input.oninput=rows;rows();
  content.append(button("Apply selected fixes",async()=>{
    const changes=eligible.filter(p=>seriesPicks.has(p.book.id)).map(p=>({
      id:p.book.id,uuid:p.book.uuid,title:p.book.title,before:p.book.hierarchy,
      after:p.after!.map((v,i)=>p.pinned[i]?p.book.hierarchy[i]:v),evidence:"",sources:[],authors:p.book.authors,identifiers:p.book.identifiers
    }));
    const chosen=[...fills.values()].filter(f=>f.synopsis||f.publisher||f.identifiers);
    if(!changes.length&&!chosen.length)throw new Error("No fixes selected.");
    if(!await confirmAction(`Apply ${changes.length} series corrections and ${chosen.length} metadata fills?`,
      `Library: ${state.data.settings.library}. Confirm the displayed source matches and editions, and close Calibre first. Only selected fixes are written. This batch has one backup, verification journal and guarded undo in History. Interrupted writes require guarded recovery.`))return;
    await act("repair_apply",{series:changes,fills:chosen},"Selected fixes applied and verified. The batch backup and guarded undo are in History.");
  },"primary"));
  const held=el("details");held.append(el("summary","Items needing attention"));
  for(const p of state.proposals.filter(p=>p.blocked&&`${p.book.title} ${p.book.authors.join(" ")}`.toLowerCase().includes(filter.input.value.toLowerCase())).slice(0,200))
    held.append(el("p",`${p.book.title}: ${p.reason}`));
  content.append(held);
}
function candidateUniverse(p:Proposal):string {
  const alias=state.data.aliases.find(a=>p.book.hierarchy.some(v=>
    v.name?.trim().toLowerCase()===a.existing_series.trim().toLowerCase()));
  return p.after?.[0].name??alias?.universe??p.book.hierarchy[0].name??"(Ungrouped)";
}
function universes() {
  title("Universe & subseries coverage","These groups cover recognized owned records only. Candidate aliases and populated metadata are not chronology evidence; complete source claims are not a guarantee of complete library coverage.");
  const groups=new Map<string,Proposal[]>();
  for(const p of state.proposals) {
    const name=candidateUniverse(p);const books=groups.get(name)??[];books.push(p);groups.set(name,books);
  }
  const search=field("Filter universe or series");
  const area=el("div");content.append(search.wrapper,area);
  const update=()=>{
    const needle=search.input.value.toLowerCase();
    const matches=[...groups].filter(([name,books])=>`${name} ${books.flatMap(p=>
      [...p.book.hierarchy,...(p.after??[])].map(v=>v.name??"")).join(" ")}`.toLowerCase().includes(needle))
      .sort((a,b)=>b[1].length-a[1].length||a[0].localeCompare(b[0]));
    area.replaceChildren(el("p",`${matches.length} groups. Showing up to 100; filter to narrow.`));
    for(const [name,books] of matches.slice(0,100)) {
      const card=el("details");
      const supported=books.filter(p=>p.reference_id&&p.after&&
        state.data.references.find(r=>r.id===p.reference_id)?.field_mask?.[0]!==false).length;
      const protectedCount=books.filter(p=>p.pinned.some(Boolean)).length;
      card.append(el("summary",`${name} · ${books.length} owned · ${supported} approved matches · ${books.length-supported} unresolved`));
      const refs=state.data.references.filter(r=>r.approved&&r.hierarchy[0].name===name);
      card.append(el("p",`${protectedCount} protected records. ${refs.some(r=>r.coverage==="complete")?
        "At least one source is marked complete by its reviewer; unmatched records still need review.":
        "No complete-universe coverage asserted; partial or unknown coverage."}`),
        button("Inspect these books",()=>{universeFilter=name;query="";status="";page=0;tab="Library";render();}));
      const branches=new Map<string,{count:number;proposed:number}>();
      for(const p of books) {
        const values=p.after??p.book.hierarchy;
        for(let slot=1;slot<4;slot++)if(values[slot].name) {
          const key=`Series${slot+1}: ${values[slot].name}`;
          const value=branches.get(key)??{count:0,proposed:0};
          value.count++;if(p.after&&p.changed.includes(slot))value.proposed++;
          branches.set(key,value);
        }
      }
      for(const [branch,count] of [...branches].sort((a,b)=>a[0].localeCompare(b[0])))
        card.append(el("p",`${branch} · ${count.count} owned records (${count.proposed} proposed changes)`));
      if(books.some(p=>p.book.hierarchy.some((v,i)=>i>0&&v.name&&!p.book.hierarchy[i-1].name)))
        card.append(el("p","Hierarchy gap detected: a populated nested field has an empty parent. Review membership; no automatic reshuffling.","warning"));
      area.append(card);
    }
  };
  search.input.oninput=update;update();
}
function audit() {
  title("Library audit","Existing series values are not automatically certified. Protected numbering is retained; unresolved works require a reviewed reference.");
  const counts=state.proposals.reduce<Record<string,number>>((a,p)=>{a[p.status]=(a[p.status]??0)+1;return a;},{});
  const stats=el("div","","stats");
  stats.append(el("div",`${state.proposals.length} books`));
  for(const [key,count] of Object.entries(counts))stats.append(el("div",`${count} ${key}`,key));
  content.append(stats);
  const tools=el("div","","toolbar"),search=field("Search title, author or series",query);
  if(universeFilter)tools.append(button(`Clear universe filter: ${universeFilter}`,()=>{universeFilter=null;render();}));
  const filter=el("select");filter.setAttribute("aria-label","Audit status");
  for(const key of ["","correct","missing","conflict","protected","unresolved"]) {
    const option=el("option",key||"All statuses");option.value=key;option.selected=status===key;filter.append(option);
  }
  tools.append(search.wrapper,filter,button("Rescan (read-only)",()=>act("scan")),
    button("Review selected",()=>{tab="Review";render();},"primary"));
  const tableArea=el("div");content.append(tools,tableArea);
  function rows() {
    const needle=query.toLowerCase();
    const filtered=state.proposals.filter(p=>(!status||p.status===status)
      && (!universeFilter||candidateUniverse(p)===universeFilter)
      && [p.book.title,...p.book.authors,candidateUniverse(p),...p.book.hierarchy.map(v=>v.name??""),
        ...(p.after??[]).map(v=>v.name??"")].join(" ").toLowerCase().includes(needle));
    const max=Math.max(0,Math.ceil(filtered.length/60)-1);page=Math.min(page,max);
    const tools=el("div","","toolbar");
    tools.append(el("span",`${filtered.length} matches · page ${page+1}/${max+1} · ${selected.size} selected`),
      button("Previous",()=>{page=Math.max(0,page-1);rows();}),
      button("Next",()=>{page=Math.min(max,page+1);rows();}),
      button("Select eligible on this page",()=>{for(const p of filtered.slice(page*60,page*60+60)) {
        const slots=p.changed.filter(i=>!p.pinned[i]);if(p.after&&!p.blocked&&slots.length)selected.set(p.book.id,new Set(slots));
      }rows();}));
    const table=el("table"),head=el("tr");
    ["Select","Book / author","Current hierarchy","Status / evidence","Rule"].forEach(t=>head.append(el("th",t)));
    table.append(head);
    for(const p of filtered.slice(page*60,page*60+60)) {
      const row=el("tr"),pick=el("td"),check=checkbox("",selected.has(p.book.id),checked=>{
        if(checked)selected.set(p.book.id,new Set(p.changed.filter(i=>!p.pinned[i])));else selected.delete(p.book.id);rows();
      });
      const input=check.querySelector("input")!;input.disabled=p.blocked||!p.changed.length||!p.after;
      input.setAttribute("aria-label",`Select ${p.book.title}`);pick.append(check);
      const book=el("td");book.append(el("strong",p.book.title),el("div",p.book.authors.join(" & "),"muted"),
        el("small",`#${p.book.id} · ${p.book.formats.join(", ")||"no ebook formats"}`));
      const values=el("td");values.append(hierarchy(p.book.hierarchy));
      const why=el("td");why.append(el("span",p.status,`badge ${p.status}`),el("div",p.reason,"reason"));
      const edit=el("td");edit.append(button("View / edit",()=>editBook(p)));
      row.append(pick,book,values,why,edit);table.append(row);
    }
    tableArea.replaceChildren(tools,table);
  }
  search.input.oninput=()=>{query=search.input.value;page=0;rows();};
  filter.onchange=()=>{status=filter.value;page=0;rows();};rows();
  if(!state.proposals.length)content.append(el("p","Choose a library and three existing series-type custom columns in Settings."));
}
function editBook(p:Proposal) {
  const dialog=el("dialog"),box=el("div","","modal wide");
  box.append(el("h2",p.book.title),el("p",p.reason),hierarchy(p.book.hierarchy),el("h3","Protected fields"));
  const mask=[...p.pinned];
  p.pinned.forEach((pinned,i)=>box.append(checkbox(`Protect Series${i===0?"":i+1}`,pinned,v=>mask[i]=v)));
  box.append(button("Save protections",async()=>{
    if(mask.some((v,i)=>!v&&p.pinned[i])&&!await confirmAction("Unlock user numbering?",
      "Only these selected fields will be unlocked. This changes the safety policy, not library metadata."))return;
    dialog.close();await act("pin",{uuid:p.book.uuid,mask});
  }));
  const current=state.data.references.find(r=>r.id===p.reference_id);
  box.append(button(current?"Edit matched reference":"Create reviewed reference",()=>{
    dialog.close();referenceEditor(current??newReference(p.book));
  }),button("Close",()=>dialog.close()));
  dialog.append(box);document.body.append(dialog);dialog.onclose=()=>dialog.remove();dialog.showModal();
}
function newReference(book?:Book):Reference {
  return {id:"",title:book?.title??"",authors:book?.authors??[],aliases:[],identifiers:book?.identifiers??{},
    book_uuids:book?[book.uuid]:[],hierarchy:book?structuredClone(book.hierarchy):
      [{name:null,index:1},{name:null,index:null},{name:null,index:null},{name:null,index:null}],
    source_id:"",evidence:"",approved:false,origin:"draft",coverage:"partial",before:[],fixed:true,edition:"any"};
}
function referenceEditor(original:Reference) {
  const r=structuredClone(original),dialog=el("dialog"),box=el("form","","modal wide");
  box.onsubmit=e=>e.preventDefault();
  box.append(el("h2",r.title||"New reference"),
    el("p","Empty selected target fields clear existing values; unchecked partial-rule fields stay unchanged. Verify whole-book edition scope and subseries numbering before approving.","warning"));
  const work=field("Work title",r.title),authors=field("Authors (separate with |)",r.authors.join(" | ")),
    aliases=field("Approved title aliases (|)",r.aliases.join(" | ")),uuids=field("Explicit bound UUIDs (|)",r.book_uuids.join(" | "));
  box.append(work.wrapper,authors.wrapper,aliases.wrapper,uuids.wrapper);
  if(r.id)box.append(el("small",`Rule ID: ${r.id}`));
  const values=r.hierarchy.map((v,i)=>{
    const group=el("div","","toolbar"),name=field(i===0?"Series: overarching universe":`Series${i+1}: genuine subseries`,v.name??""),
      number=field("Independent number",v.index?.toString()??"","number");
    number.input.step="any";group.append(name.wrapper,number.wrapper);box.append(group);return {name,number};
  });
  if(r.field_mask) {
    box.append(el("p","This is a partial-field rule: unchecked fields stay unchanged on every matched record.","warning"));
    r.field_mask.forEach((enabled,i)=>box.append(checkbox(`Use Series${i===0?"":i+1} in this reference`,enabled,v=>r.field_mask![i]=v)));
  }
  if(r.ai_provenance) {
    box.append(el("p",`AI-assisted provenance: ${r.ai_provenance.model} · ${r.ai_provenance.digest}. Literal citations are checked, not factual correctness.`,"warning"),
      inspectEvidence(r.ai_provenance.citations,state.data.sources.find(s=>s.id===r.source_id)));
  }
  const source=el("select");source.append(el("option","New local user decision"));
  source.options[0].value="";
  for(const s of state.data.sources) {const option=el("option",`${s.kind}: ${s.name}`);option.value=s.id;option.selected=s.id===r.source_id;source.append(option);}
  const sourceLabel=el("label");sourceLabel.append(el("span","Provenance"),source);box.append(sourceLabel);
  const evidence=el("textarea");evidence.value=r.evidence;evidence.rows=5;
  const proof=el("label");proof.append(el("span","Evidence / explicit user decision"),evidence);box.append(proof);
  const identifiers=el("textarea");identifiers.value=JSON.stringify(r.identifiers,null,2);identifiers.rows=3;
  const ids=el("label");ids.append(el("span","Edition identifiers (JSON object)"),identifiers);box.append(ids);
  const before=el("select");before.multiple=true;before.size=5;
  for(const other of state.data.references.filter(other=>other.id!==r.id)) {
    const option=el("option",`${other.title} — ${series(other.hierarchy[0])}`);
    option.value=other.id;option.selected=r.before.includes(other.id);before.append(option);
  }
  const constraints=el("label");constraints.append(el("span","This work occurs before (Ctrl-click for multiple; same universe only)"),before);box.append(constraints);
  const scope=el("select");for(const name of ["any","novel","collection","omnibus"]) {
    const option=el("option",name);option.selected=name===r.edition;scope.append(option);
  }
  const scopeLabel=el("label");scopeLabel.append(el("span","Verified edition scope"),scope);box.append(scopeLabel);
  box.append(checkbox("Fixed chronological anchor (otherwise calculate from before relations)",r.fixed,v=>r.fixed=v),
    checkbox("I reviewed this evidence and approve this rule",r.approved,v=>r.approved=v),
    checkbox("Source covers the complete universe, not a partial owned subset",r.coverage==="complete",v=>r.coverage=v?"complete":"partial"));
  box.append(button("Save reference",async()=>{
    const split=(value:string)=>value.split("|").map(v=>v.trim()).filter(Boolean);
    r.title=work.input.value.trim();r.authors=split(authors.input.value);r.aliases=split(aliases.input.value);
    r.book_uuids=split(uuids.input.value);r.source_id=source.value;r.evidence=evidence.value.trim();
    r.identifiers=JSON.parse(identifiers.value);r.edition=scope.value;r.before=Array.from(before.selectedOptions,o=>o.value);
    r.hierarchy=values.map(({name,number},i)=>({name:name.input.value.trim()||null,
      index:number.input.value===""?(i===0&&!name.input.value.trim()?1:null):Number(number.input.value)})) as Hierarchy;
    await act("save_reference",r,"Reference saved locally. Review its proposed field changes before applying.");dialog.close();
  },"primary"),button("Cancel",()=>dialog.close()));
  if(r.id)box.append(button("Delete rule",async()=>{
    if(await confirmAction("Delete reference?","This deletes the local rule, not any Calibre metadata.")) {
      await act("delete_reference",{id:r.id});dialog.close();
    }
  },"danger"));
  dialog.append(box);document.body.append(dialog);dialog.onclose=()=>dialog.remove();dialog.showModal();
}
function references() {
  title("References & sources","Research imports are drafts. Rules are reusable locally across future batches; no paid provider or background fetching is used.");
  const tools=el("div","","toolbar");
  tools.append(button("Import JSON / CSV / ODT / HTML",async()=>{
    const location=await open({multiple:false,filters:[{name:"Sources",extensions:["json","csv","odt","html","htm"]}]});
    if(typeof location==="string")importDialog(location,false);
  }),button("Fetch explicit HTTPS page",()=>importDialog("",true)),button("New reference",()=>referenceEditor(newReference())),
    button("Import verified batch history",async()=>{
      const location=await open({directory:true,multiple:false});
      if(typeof location==="string")await act("seed",{location},"Imported latest verified targets per UUID. No Calibre writes.");
    }),button("Export reusable rules",async()=>{
      const location=await save({defaultPath:"curator-rules.json",filters:[{name:"JSON",extensions:["json"]}]});
      if(location)await act("export",{location},"Rules exported. Imported copies will require review again.");
    }));
  content.append(tools);
  const order=el("div","","toolbar"),universe=field("Universe for constraint ordering");
  order.append(universe.wrapper,button("Calculate draft positions",async()=>{
    if(await confirmAction("Calculate relative order?","Fixed anchors remain unchanged. Incomplete order and cycles are rejected; changed positions become drafts for review.")) {
      await act("sequence",{universe:universe.input.value.trim()},"Calculated positions saved as drafts; review and approve them.");
    }
  }));content.append(order);
  const search=field("Filter references");content.append(search.wrapper);
  const area=el("div");content.append(area);
  const update=()=>{
    const filtered=state.data.references.filter(r=>`${r.title} ${r.hierarchy[0].name??""}`.toLowerCase().includes(search.input.value.toLowerCase()));
    const table=el("table"),head=el("tr");["Work","Target hierarchy","Review","Action"].forEach(t=>head.append(el("th",t)));table.append(head);
    for(const r of filtered.slice(0,300)) {
      const row=el("tr"),name=el("td");name.append(el("strong",r.title),el("div",r.authors.join(" & ")));
      const values=el("td");values.append(hierarchy(r.hierarchy));
      const proof=el("td");proof.append(el("span",r.approved?"approved":"draft",`badge ${r.approved?"correct":"unresolved"}`),
        el("div",state.issues[r.id]??r.evidence,"reason"));
      const edit=el("td");edit.append(button("Edit",()=>referenceEditor(r)));row.append(name,values,proof,edit);table.append(row);
    }
    area.replaceChildren(el("p",`${filtered.length} references (${filtered.filter(r=>r.approved).length} approved). Showing up to 300; filter to narrow.`),table);
  };search.input.oninput=update;update();
  const sources=el("details");sources.append(el("summary",`Source cache (${state.data.sources.length})`));
  for(const source of state.data.sources) {
    const item=el("details");item.append(el("summary",`${source.kind}: ${source.name}`),
      el("p",source.location),el("small",`SHA-256: ${source.sha256}`));
    const text=el("pre");text.textContent=source.segments.map(s=>`${s.ordinal} [${s.kind}] ${s.text}`).join("\n");
    item.append(text);sources.append(item);
  }content.append(sources);
  const aliasBox=el("details");aliasBox.append(el("summary","Candidate universe aliases (not chronology evidence)"));
  const editor=el("textarea");editor.rows=7;editor.value=JSON.stringify(state.data.aliases,null,2);
  aliasBox.append(el("p","Map existing series labels to candidate universes without writing or inventing chronological positions. Fields: id, existing_series, universe."),
    editor,button("Save aliases",()=>act("aliases",JSON.parse(editor.value))));content.append(aliasBox);
}
function importDialog(location:string,remote:boolean) {
  const dialog=el("dialog"),box=el("div","","modal"),path=field(remote?"Public HTTPS URL":"Local source",location),
    universe=field("Universe for numbered prose extraction (optional)");
  box.append(el("h2",remote?"Fetch source":"Import source"),path.wrapper,universe.wrapper,
    el("p","ODT headings and table rows are preserved. Parsed rules remain drafts; prose does not establish the full nested hierarchy."),
    button("Import as research",async()=>{await act("import",{location:path.input.value,universe:universe.input.value,remote});dialog.close();},"primary"),
    button("Cancel",()=>dialog.close()));
  dialog.append(box);document.body.append(dialog);dialog.onclose=()=>dialog.remove();dialog.showModal();
}
function review() {
  title("Field-level review","Only checked fields are written. Close Calibre first. Every apply operation receives its own database/OPF backup and journal.");
  const proposals=state.proposals.filter(p=>selected.has(p.book.id)&&p.after);
  content.append(el("p",`${proposals.length} books selected. Backup workspace: ${state.workspace}/operations`));
  for(const p of proposals) {
    const card=el("section","","card");card.append(el("h2",p.book.title),el("p",p.reason,"reason"));
    const table=el("table"),head=el("tr");["Apply","Field","Current","Proposed"].forEach(t=>head.append(el("th",t)));table.append(head);
    for(let i=0;i<4;i++) {
      const row=el("tr"),cell=el("td"),check=checkbox("",selected.get(p.book.id)?.has(i)??false,checked=>{
        if(checked)selected.get(p.book.id)?.add(i);else selected.get(p.book.id)?.delete(i);
      });
      check.querySelector("input")!.disabled=!p.changed.includes(i)||p.pinned[i];cell.append(check);
      row.append(cell,el("td",`Series${i===0?"":i+1}${p.pinned[i]?" (protected)":""}`),
        el("td",series(p.book.hierarchy[i])),el("td",series(p.after![i])));table.append(row);
    }card.append(table);content.append(card);
  }
  content.append(button("Apply checked changes",async()=>{
    const changes=proposals.filter(p=>(selected.get(p.book.id)?.size??0)>0).map(p=>({
      id:p.book.id,uuid:p.book.uuid,title:p.book.title,before:p.book.hierarchy,
      after:p.after!.map((v,i)=>selected.get(p.book.id)?.has(i)?v:p.book.hierarchy[i]),
      evidence:"",sources:[],authors:p.book.authors,identifiers:p.book.identifiers
    }));
    if(!changes.length)throw new Error("No changed fields selected.");
    if(!await confirmAction(`Apply metadata changes to ${changes.length} books?`,
      `Library: ${state.data.settings.library}. Confirm Calibre is closed. Backups: ${state.workspace}/operations. This writes Calibre metadata, not a reading list.`))return;
    await act("apply",changes,"Applied and verified. Database integrity, all-book hierarchy, OPFs and unrelated metadata checked.");
    tab="History";render();
  },"primary"),button("Clear selection",()=>{selected.clear();render();}));
}
function history() {
  title("History & recovery","Undo/recovery uses Calibre’s API, never a blind database restore. Intervening edits or an uncheckpointed write block automatic reversal.");
  for(const op of state.history) {
    const card=el("section","","card");
    card.append(el("h2",`${op.metadata_field??"series"} · ${op.kind} · ${op.phase} · ${op.changes.length} books`),
      el("p",`${new Date(op.created*1000).toLocaleString()} · ${op.library}`),el("small",op.id),
      el("p",`Backup: ${op.backup??"not created"}`));
    if(op.error)card.append(el("p",op.error,"error"));
    const details=el("details");details.append(el("summary","Journal / verification"));
    const json=el("pre",JSON.stringify(op,null,2));details.append(json);card.append(details);
    if(!["rejected","undone","recovered"].includes(op.phase))card.append(button(op.phase==="verified"?"Guarded undo":"Recover interrupted operation",async()=>{
      if(await confirmAction("Guarded reversal?","Close Calibre. All targets must still match the saved checkpoint. Protected fields remain protected; explicitly unlock only if you intend to reverse newly protected membership.")) {
        await act("reverse",{id:op.id},"Guarded reversal applied and verified.");
      }
    }));
    if(!op.backup&&["prepared","locked"].includes(op.phase))card.append(button("Dismiss pre-write failure",()=>act("dismiss",{id:op.id})));
    content.append(card);
  }
  if(!state.history.length)content.append(el("p","No writes performed by this application."));
}
function diagnostics() {
  title("Secondary metadata diagnostics","These checks do not rewrite comments, publishers, identifiers or ebook files.");
  const fields:[string,(b:Book)=>boolean][]=[
    ["Missing synopsis/comments",b=>!b.has_comments],["Missing publisher",b=>!b.publisher?.trim()],
    ["Missing identifiers",b=>!Object.keys(b.identifiers).length],["No ebook format",b=>!b.formats.length]
  ];
  for(const [label,test] of fields) {
    const books=state.proposals.map(p=>p.book).filter(test),details=el("details");
    details.append(el("summary",`${label}: ${books.length}`));
    for(const book of books)details.append(el("p",`#${book.id} ${book.title} — ${book.authors.join(" & ")}`));
    content.append(details);
  }
}
function settings() {
  title("Settings","Series2–4 must map to three distinct, existing series-type custom columns. The application never creates or migrates library columns.");
  const s=state.data.settings,library=field("Calibre library directory",s.library),
    binary=field("Absolute installed calibre-debug",s.calibre_binary);
  content.append(library.wrapper,button("Choose library",async()=>{
    const path=await open({directory:true,multiple:false});if(typeof path==="string")library.input.value=path;
  }),binary.wrapper);
  const columns=s.columns.map((value,i)=>field(`Series${i+2} custom-column label (without #)`,value));
  columns.forEach(c=>content.append(c.wrapper));
  const available=el("p","","muted"),options=el("datalist");options.id="series-columns";content.append(available,options);
  columns.forEach(c=>c.input.setAttribute("list",options.id));
  const discover=async()=>{
    try {
      const result=await invoke<{label:string;name:string}[]>("discover_columns",{library:library.input.value});
      options.replaceChildren(...result.map(c=>{const option=el("option",c.name);option.value=c.label;return option;}));
      available.textContent=`Compatible columns: ${result.map(c=>`${c.label} (${c.name})`).join(", ")||"none"}`;
    }catch(error){available.textContent=String(error);}
  };
  library.input.onchange=discover;content.append(button("Discover compatible columns",discover));void discover();
  content.append(button("Validate, save and scan",()=>act("settings",{
    library:library.input.value,calibre_binary:binary.input.value,columns:columns.map(c=>c.input.value.trim())
  }),"primary"),el("p",`Application workspace: ${state.workspace}`,"muted"),
  el("p","No credentials, AI API keys, Calibre passwords or paid-provider configuration are needed."));
}

function synopses() {
  title("Metadata lookup & synopses","Search selected Calibre providers, confirm the work/edition, then create and approve a synopsis draft. Existing Comments and all series fields remain protected.");
  const config=structuredClone(state.data.metadata_settings),settingsBox=el("details");
  const available=state.data.metadata_providers.filter(p=>p.enabled&&p.identify);
  const scope=config.search_all?`All enabled identification providers${available.length?` (${available.length} last discovered)`:" — discovered at lookup"}`:
    `Custom provider selection: ${config.providers.join(", ")}`;
  content.append(el("p",`Metadata search: ${scope}. Web search: DuckDuckGo.`,"warning"));
  settingsBox.open=state.data.metadata_providers.length===0;
  settingsBox.append(el("summary","Metadata provider settings"));
  const binary=field("Absolute fetch-ebook-metadata executable",config.fetch_binary),
    timeout=field("Per-provider timeout (5–30 seconds)",String(config.timeout_seconds),"number"),
    days=field("Lookup cache days (0 disables reuse; maximum 30)",String(config.cache_days),"number"),
    providers=el("select");
  providers.multiple=true;providers.size=7;
  const catalogue=state.data.metadata_providers.length?state.data.metadata_providers:
    config.providers.map(name=>({name,version:"discover to verify",identify:true,enabled:true}));
  for(const p of catalogue) {
    const option=el("option",`${p.name} · ${p.version}${!p.identify?" · covers only":!p.enabled?" · disabled in Calibre":""}`);
    option.value=p.name;option.selected=config.search_all?(p.enabled&&p.identify):config.providers.includes(p.name);option.disabled=!p.identify||!p.enabled;providers.append(option);
  }
  providers.disabled=config.search_all;
  const providerLabel=el("label");providerLabel.append(el("span","Custom selection (no three-source limit; Ctrl-click for multiple)"),providers);
  settingsBox.append(checkbox("Search ALL enabled identification providers",config.search_all,value=>{
    config.search_all=value;providers.disabled=value;
    for(const option of Array.from(providers.options))option.selected=value?!option.disabled:config.providers.includes(option.value);
  }),binary.wrapper,timeout.wrapper,days.wrapper,providerLabel,
    button("Discover Calibre providers",()=>act("metadata_discover",{},"Provider capabilities and enabled status loaded; no lookup performed.")),
    button("Save metadata settings",()=>{
      config.fetch_binary=binary.input.value.trim();config.timeout_seconds=Number(timeout.input.value);
      config.cache_days=Number(days.input.value);config.providers=Array.from(providers.selectedOptions,o=>o.value);
      return act("metadata_settings",config,"Metadata settings saved. Credentials remain managed by Calibre.");
    }),
    el("p","Calibre's existing plugin configuration is reused. Configure credentials or enable providers in Calibre; Curator does not request or copy API keys. Raw plugin diagnostics are discarded."));
  content.append(settingsBox);
  const eligible=state.proposals.map(p=>p.book).filter(b=>!b.authors.some(a=>normalized(a)==="calibre")&&
    !(b.formats.length>0&&b.formats.every(f=>f==="CSV"||f==="XML")));
  const missing=eligible.filter(b=>!b.has_comments);
  content.append(el("p",`${missing.length} book records have blank Comments; catalogue/export records are excluded. Metadata-only records can be looked up, but matching a filename or title alone does not establish identity.`));
  if(!eligible.some(b=>b.id===synopsisBook))synopsisBook=missing[0]?.id??0;
  const search=field("Find a book (title or author)"),books=el("select"),bookInfo=el("div"),results=el("div");
  function bookOptions() {
    const needle=search.input.value.toLowerCase(),empty=el("option","Choose a book");empty.value="";books.replaceChildren(empty);
    const matches=eligible.filter(b=>(!b.has_comments||b.id===synopsisBook)&&
      `${b.title} ${b.authors.join(" ")}`.toLowerCase().includes(needle)).slice(0,100);
    for(const b of matches) {
      const option=el("option",`#${b.id} ${b.title} — ${b.authors.join(" & ")}${b.has_comments?" · Comments populated/protected":""}`);
      option.value=String(b.id);option.selected=b.id===synopsisBook;books.append(option);
    }
  }
  function showBook() {
    bookInfo.replaceChildren();
    const book=eligible.find(b=>b.id===synopsisBook);
    if(book)bookInfo.append(el("strong",book.title),el("p",book.authors.join(" & ")),
      el("p",`Identifiers: ${JSON.stringify(book.identifiers)} · Formats: ${book.formats.join(", ")||"no local ebook format"}`),
      el("p",book.has_comments?"Comments are populated: generation/apply will refuse to overwrite them.":"Comments are blank; any description still requires source and text approval.",book.has_comments?"warning":"muted"));
    lookupCards();draftCards();
  }
  search.input.oninput=bookOptions;books.onchange=()=>{synopsisBook=Number(books.value);showBook();};
  bookOptions();
  const lookup=async(refresh:boolean)=>{
    const book=eligible.find(b=>b.id===synopsisBook);if(!book)throw new Error("Choose a book first.");
    if(!await confirmAction("Search online metadata sources?",
      `Send the title, authors and recognized identifiers for "${book.title}" to ${state.data.metadata_settings.search_all?"ALL enabled Calibre identification providers":state.data.metadata_settings.providers.join(", ")}. Every provider is searched separately even if another fails or finds a result. No ebook files or inventory are sent; a full search may take several minutes.`))return;
    await act("metadata_lookup",{book_id:book.id,refresh},"Lookup finished. Review each provider separately; no identity was automatically confirmed.");
  };
  content.append(search.wrapper,books,bookInfo,button(config.search_all?"Search all metadata providers":"Look up selected book",()=>lookup(false),"primary"),
    button("Refresh lookup (ignore cache)",()=>lookup(true)),
    button("Search web with DuckDuckGo",async()=>{
      const book=eligible.find(b=>b.id===synopsisBook);if(!book)throw new Error("Choose a book first.");
      if(!await confirmAction("Open DuckDuckGo web search?",
        `Search for "${book.title}", its authors and ISBN in your default browser. This is source discovery only: results/snippets are not automatically imported, trusted or used to generate metadata.`))return;
      await act("metadata_web_search",{book_id:book.id},"DuckDuckGo opened in your browser. Verify original source pages; search snippets are not evidence.");
    }),results);
  const drafts=el("div");content.append(drafts);
  function lookupCards() {
    results.replaceChildren(el("h2","Provider results"));
    const entries=state.data.metadata_results.filter(r=>r.library===state.data.settings.library&&r.book_id===synopsisBook);
    for(const result of entries) {
      const card=el("section","","card"),metadata=result.metadata;
      card.append(el("h3",`${result.provider.name} · ${result.match_status} · ${result.cached?"cached":"fetched"}`),
        el("p",`${new Date(result.fetched_at*1000).toLocaleString()} · provider version ${result.provider.version}`),el("p",result.note,"warning"));
      if(metadata) {
        card.append(el("strong",metadata.title),el("p",metadata.authors.join(" & ")),
          el("p",`Publisher: ${metadata.publisher??"not supplied"} · IDs: ${JSON.stringify(metadata.identifiers)}`),
          el("p",metadata.url??"No stable public source URL returned; provider/query/time/hash provenance is retained."),
          el("pre",metadata.description||"No description was supplied."));
        if(entries.some(other=>other.id!==result.id&&other.metadata?.description&&
          normalized(other.metadata.description)===normalized(metadata.description)))
          card.append(el("p","Another provider returned the same description. This is not independent corroboration.","warning"));
        if(result.match_status==="compatible")card.append(
          button(result.identity_confirmed?"Revoke work/edition confirmation":"Confirm matching work and edition",async()=>{
            if(!result.identity_confirmed&&!await confirmAction("Confirm source identity?",
              "Compare title, authors, identifiers, edition and collection scope. Where the ebook is ambiguous or misattributed, establish its actual identity first. A provider match is not a factual guarantee."))return;
            await act("metadata_confirm",{id:result.id,confirmed:!result.identity_confirmed},"Source confirmation saved. No synopsis approved or metadata written.");
          }));
        if(result.identity_confirmed&&result.match_status==="compatible"&&metadata.description.length>=80) {
          const generate=async(ollama:boolean,refresh=false)=>{
            if(ollama&&!state.data.ai_settings.enabled)throw new Error("Enable and configure local Ollama in AI assistant first, or reuse the provider description.");
            if(!await confirmAction("Create an unapproved synopsis draft?",ollama?
              "Send this reviewed description to the configured local/private Ollama server. Unsupported facts and model memory are not permitted; inspect citations before approval.":
              "Retain this provider description as a draft. It is not automatically approved or applied."))return;
            await act("synopsis_generate",{id:result.id,ollama,refresh},"Unapproved synopsis result saved. Review the text and citations before approval.");
          };
          card.append(button("Use source description as draft",()=>generate(false)),
            button("Draft synopsis with Ollama",()=>generate(true)),
            button("Regenerate synopsis (ignore AI cache)",()=>generate(true,true)));
        }
      }results.append(card);
    }
    if(!entries.length)results.append(el("p","No lookup results for this book."));
  }
  function draftCards() {
    drafts.replaceChildren(el("h2","Synopsis drafts"));
    for(const draft of state.data.synopsis_drafts.filter(d=>d.library===state.data.settings.library&&
      (synopsisBook===0||d.book_id===synopsisBook)).slice(0,50)) {
      const card=el("section","","card");card.append(el("h3",`${draft.title} · ${draft.state}`),
        el("p",`${draft.model} · ${draft.cached?"cache hit":"new draft"} · ${draft.output_tokens} generated tokens`),
        el("p",draft.notes),inspectEvidence(draft.citations,state.data.sources.find(s=>s.id===draft.source_id)));
      if(draft.state!=="insufficient_evidence"&&draft.state!=="applied") {
        const text=el("textarea");text.rows=7;text.value=draft.text;
        const label=el("label");label.append(el("span","Synopsis text (plain text; HTML is escaped when applied)"),text);
        let approved=draft.approved;
        card.append(label,checkbox("I reviewed this synopsis against the source and approve it",approved,v=>approved=v),
          button("Save synopsis review",()=>act("synopsis_save",{id:draft.id,text:text.value,approved},"Synopsis review saved. Apply remains a separate action.")));
      } else card.append(el("p",draft.text||"No supported synopsis was generated."));
      if(draft.approved&&draft.state==="approved")card.append(checkbox("Select this approved synopsis for apply",synopsisSelected.has(draft.id),
        value=>{if(value)synopsisSelected.add(draft.id);else synopsisSelected.delete(draft.id);}));
      drafts.append(card);
    }
    drafts.append(button("Apply selected approved synopses",async()=>{
      const selected=state.data.synopsis_drafts.filter(d=>synopsisSelected.has(d.id)&&d.library===state.data.settings.library&&d.approved&&d.state==="approved");
      if(!selected.length)throw new Error("Select at least one approved synopsis.");
      if(!await confirmAction(`Fill blank Comments for ${selected.length} book(s)?`,
        `Library: ${state.data.settings.library}. Close Calibre normally first. Only blank Comments are filled; existing Comments and series values are preserved. Backups/journals: ${state.workspace}/operations.`))return;
      await act("synopsis_apply",selected.map(d=>d.id),"Synopses applied and verified through Calibre. All-book hierarchy, Comments, OPFs and unrelated metadata checked.");
      synopsisSelected.clear();tab="History";render();
    },"primary"));
  }
  showBook();
}
function normalized(value:string) {return value.normalize("NFD").replace(/\p{M}/gu,"").toLowerCase().replace(/[^\p{L}\p{N}]/gu,"");}
function inspectEvidence(citations:Citation[],source:Source|undefined) {
  const details=el("details");details.append(el("summary","Inspect exact cited source text"));
  for(const citation of citations) {
    details.append(el("p",`Segment ${citation.segment}`),el("blockquote",citation.quote));
    const segment=source?.segments.find(s=>s.ordinal===citation.segment);
    const full=el("details");full.append(el("summary","Full cached segment"),el("pre",segment?.text??"Source segment unavailable"));
    details.append(full);
  }
  if(source)details.append(el("small",`Source SHA-256: ${source.sha256}`));
  return details;
}
function aiWorkspace() {
  title("Optional Ollama assistant","Local, source-grounded suggestions only. No automatic downloads, cloud fallback, approval, protection changes or Calibre writes.");
  const config=structuredClone(state.data.ai_settings),settingsBox=el("section","","card"),
    endpoint=field("Ollama origin",config.endpoint),model=field("Installed model name",config.model),
    timeout=field("Request timeout (15–180 seconds)",String(config.timeout_seconds),"number");
  const models=el("datalist");models.id="ollama-models";
  for(const m of state.data.ai_models.filter(m=>m.usable)) {
    const option=el("option",`${(m.size/1e9).toFixed(1)} GB`);option.value=m.name;models.append(option);
  }
  model.input.setAttribute("list",models.id);
  settingsBox.append(el("h2","Connection & privacy"),endpoint.wrapper,model.wrapper,models,timeout.wrapper,
    checkbox("Enable optional Ollama suggestions",config.enabled,v=>config.enabled=v),
    checkbox("Allow a private LAN/VPN server: selected excerpts leave this computer",config.allow_lan,v=>config.allow_lan=v),
    button("Save AI settings",async()=>{
      config.endpoint=endpoint.input.value.trim();config.model=model.input.value.trim();config.timeout_seconds=Number(timeout.input.value);
      if(config.allow_lan&&!await confirmAction("Allow private-server requests?","Only explicitly selected source excerpts are sent. Trust the server operator; do not expose Ollama publicly."))return;
      await act("ai_settings",config,"AI settings saved. No inference request was made.");
    }),button("List installed models",()=>act("ai_models",{},"Model catalogue refreshed; no inference or downloads performed.")));
  for(const m of state.data.ai_models)settingsBox.append(el("p",`${m.name} · ${(m.size/1e9).toFixed(2)} GB · ${m.usable?"available":m.reason}`,"muted"));
  content.append(settingsBox);
  const box=el("section","","card"),sources=el("select");
  if(!state.data.sources.some(s=>s.id===aiSelection.source_id))aiSelection.source_id=state.data.sources.find(s=>s.kind==="odt")?.id??state.data.sources[0]?.id??"";
  for(const s of state.data.sources) {const option=el("option",`${s.kind}: ${s.name}`);option.value=s.id;option.selected=s.id===aiSelection.source_id;sources.append(option);}
  const sourceLabel=el("label");sourceLabel.append(el("span","Cached source (import in References & sources first)"),sources);
  const start=field("First segment",String(aiSelection.start),"number"),end=field("Last segment",String(aiSelection.end),"number"),
    universe=field("User-selected universe context (optional)",aiSelection.universe);
  const preview=el("pre"),count=el("p");
  const update=()=>{
    aiSelection={source_id:sources.value,start:Number(start.input.value),end:Number(end.input.value),universe:universe.input.value.trim()};
    const source=state.data.sources.find(s=>s.id===sources.value);
    const segments=source?.segments.filter(s=>s.ordinal>=aiSelection.start&&s.ordinal<=aiSelection.end)??[];
    const bytes=new TextEncoder().encode(segments.map(s=>s.text).join("")).length;
    count.textContent=`${segments.length} selected segments · ${bytes}/12,000 UTF-8 bytes (maximum 32 segments). Only these texts and the context are sent.`;
    preview.textContent=segments.map(s=>`${s.ordinal} [${s.kind}] ${s.text}`).join("\n\n");
  };
  sources.onchange=()=>{start.input.value="1";end.input.value=String(Math.min(3,state.data.sources.find(s=>s.id===sources.value)?.segments.at(-1)?.ordinal??1));update();};
  start.input.oninput=update;end.input.oninput=update;universe.input.oninput=update;
  box.append(el("h2","Preview exact source selection"),sourceLabel,start.wrapper,end.wrapper,universe.wrapper,count,preview);
  const extract=async(refresh:boolean)=>{
    update();
    if(!state.data.ai_settings.enabled)throw new Error("Enable and save optional Ollama assistance first.");
    if(!await confirmAction("Request cited draft suggestions?",
      `Use ${state.data.ai_settings.model} at ${state.data.ai_settings.endpoint} for the previewed excerpts only. Cached results avoid repeat generation; literal citations do not prove chronology.`))return;
    await act("ai_extract",{...aiSelection,refresh},"Suggestions validated and saved to the inbox. No rule approved or Calibre metadata written.");
  };
  box.append(button("Extract cited drafts",()=>extract(false),"primary"),button("Regenerate (ignore cache)",()=>extract(true)));
  content.append(box);update();
  const inbox=el("section");inbox.append(el("h2","AI review inbox"),
    el("p","Keep a suggestion pending, dismiss it, or create an unapproved reference. Approve and apply later in the normal workflow. Cached inbox results remain reviewable when Ollama is offline."));
  for(const run of state.data.ai_runs) {
    const card=el("details");card.open=run===state.data.ai_runs[0];
    const source=state.data.sources.find(s=>s.id===run.source_id);
    card.append(el("summary",`${source?.name??"Source unavailable"} · ${run.model} · ${run.cached?"cache hit":"generated"} · ${run.extraction.status}`),
      el("p",run.extraction.explanation),el("small",`${new Date(run.generated_at*1000).toLocaleString()} · ${run.input_bytes} input bytes · ${run.prompt_tokens} prompt / ${run.output_tokens} output tokens · ${(run.duration_ms/1000).toFixed(1)}s generation`),
      el("small",`Model digest: ${run.digest} · ${run.prompt_version}`));
    for(const warning of run.extraction.warnings)card.append(el("p",warning,"warning"));
    if(run.identity_context_expansions)card.append(el("p",`Curator expanded ${run.identity_context_expansions} identity citation(s) to exact context from already-cited segments. This is backend-added context, not an additional model quotation or independent corroboration.`,"warning"));
    run.extraction.candidates.forEach((candidate,index)=>{
      const item=el("section","","card"),decision=state.data.ai_decisions[run.fingerprints[index]],draft=state.data.references.find(r=>r.id===decision?.reference_id);
      item.append(el("h3",candidate.title),el("p",`${candidate.authors.join(" & ")||"Author not supplied"} · ${candidate.edition} · ${decision?.status??"pending"}`),
        el("p",candidate.notes),el("p","Literal quote checks passed. This is not a factual-confidence score or approval.","warning"));
      for(const f of candidate.fields)item.append(el("p",`Series${f.slot===0?"":f.slot+1}: ${f.name} · ${f.index??"number missing—review required"}`));
      item.append(inspectEvidence([...candidate.citations,...candidate.fields.flatMap(f=>f.citations)],source));
      const existing=state.data.references.filter(r=>normalized(r.title)===normalized(candidate.title));
      if(existing.length)item.append(el("p",`${existing.length} existing rule(s) have this normalized title. Compare targets before adding another rule.`,"warning"));
      if(draft)item.append(button("Open existing reference",()=>referenceEditor(draft)));
      else if(decision?.status==="dismissed")item.append(button("Restore suggestion",()=>act("ai_decide",{run_id:run.id,index,status:"pending"})));
      else item.append(button("Create unapproved reference",()=>bindAiDraft(run,index,candidate)),
        button("Dismiss and remember",()=>act("ai_decide",{run_id:run.id,index,status:"dismissed"})));
      card.append(item);
    });inbox.append(card);
  }
  inbox.append(button("Clear AI response cache",async()=>{
    if(await confirmAction("Clear cached AI results?","This removes response-cache/inbox entries, not sources, references, approved rules or remembered decisions."))await act("ai_clear_cache");
  }));
  content.append(inbox);reviewTriage();
}
function bindAiDraft(run:AiRun,index:number,candidate:AiCandidate) {
  const dialog=el("dialog"),box=el("div","","modal wide"),search=field("Find a library record (optional)",candidate.title),select=el("select");
  const renderMatches=()=>{
    const empty=el("option","No binding — use source identity only");empty.value="";select.replaceChildren(empty);
    const needle=search.input.value.toLowerCase();
    const books=state.proposals.map(p=>p.book).filter(b=>`${b.title} ${b.authors.join(" ")}`.toLowerCase().includes(needle));
    for(const book of books.slice(0,50)) {const option=el("option",`#${book.id} ${book.title} — ${book.authors.join(" & ")}`);option.value=String(book.id);select.append(option);}
  };
  search.input.oninput=renderMatches;renderMatches();
  const current=el("div");select.onchange=()=>{
    const book=state.proposals.find(p=>p.book.id===Number(select.value))?.book;
    current.replaceChildren();if(book)current.append(el("p",`${book.title} — ${book.authors.join(" & ")} · ${book.formats.join(", ")}`),hierarchy(book.hierarchy));
  };
  box.append(el("h2",`Create draft: ${candidate.title}`),el("p","A library binding requires your explicit selection. Unspecified fields stay unchanged, even without a binding. No suggestion can unlock protected fields.","warning"),
    search.wrapper,select,current,button("Create draft only",async()=>{
      await act("ai_draft",{run_id:run.id,index,book_id:select.value?Number(select.value):null},"Unapproved draft created. Review it in References before approving or applying.");dialog.close();
    },"primary"),button("Cancel",()=>dialog.close()));
  dialog.append(box);document.body.append(dialog);dialog.onclose=()=>dialog.remove();dialog.showModal();
}
function reviewTriage() {
  const queue=el("details");queue.append(el("summary","Review priority queue — no AI calls"));
  const drafts=new Map<string,Reference[]>();
  for(const r of state.data.references.filter(r=>!r.approved)) {const key=normalized(r.title),items=drafts.get(key)??[];items.push(r);drafts.set(key,items);}
  const rows=state.proposals.map(p=>{
    const reasons:string[]=[];let score=0;
    if(p.status==="conflict"){reasons.push("Approved rule conflicts with current metadata");score+=10;}
    if(p.reason.includes("Conflicting approved references")||(p.reference_id&&state.issues[p.reference_id])) {
      reasons.push("Approved references or chronology constraints conflict");score+=12;
    }
    const matches=(drafts.get(normalized(p.book.title))??[]).filter(r=>!r.authors.length||!p.book.authors.length||
      r.authors.some(a=>p.book.authors.some(b=>normalized(a)===normalized(b))));
    if(matches.length){reasons.push(`${matches.length} draft reference(s) available`);score+=6;}
    if(p.book.hierarchy.some((v,i)=>i>0&&v.name&&!p.book.hierarchy[i-1].name)){reasons.push("Nested series has an empty parent");score+=4;}
    if(/\b(omnibus|collection|boxed set|box set)\b/i.test(p.book.title)){reasons.push("Edition scope needs careful review");score+=2;}
    if(!p.book.authors.length){reasons.push("Author identity missing");score+=2;}
    return {p,matches,reasons,score};
  }).filter(r=>r.score).sort((a,b)=>b.score-a.score||a.p.book.id-b.p.book.id);
  queue.append(el("p",`${rows.length} records with review signals. Showing the first 100; ranking indicates review value, not factual confidence.`));
  for(const {p,matches,reasons} of rows.slice(0,100)) {
    const item=el("div","","card");item.append(el("strong",`#${p.book.id} ${p.book.title}`),el("p",reasons.join(" · ")),
      button("Inspect record",()=>editBook(p)));
    for(const ref of matches.slice(0,3))item.append(button(`Review draft: ${ref.title}`,()=>referenceEditor(ref)));
    queue.append(item);
  }content.append(queue);
}

async function start() {
  try {
    await listen<string>("curator-progress",event=>{
      try {
        const update=JSON.parse(event.payload) as {phase:string;completed:number;total:number};
        if(busy)message(`Calibre: ${update.phase} · ${update.completed}/${update.total} books. Keep this window open.`);
      }catch{/* Plugin output cannot become executable UI content. */}
    });
    state=await invoke<Snapshot>("dispatch",{action:"snapshot",payload:{}});
    render();
    await act("scan",{},"Read-only scan complete. No Calibre metadata was changed.");
  } catch(error) {if(state)render();message(String(error),true);}
}
void start();
