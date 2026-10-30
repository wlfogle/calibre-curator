import { invoke } from "@tauri-apps/api/core";
import { open, save } from "@tauri-apps/plugin-dialog";
import "./style.css";

type Series = { name: string | null; index: number | null };
type Hierarchy = [Series, Series, Series, Series];
type Book = { id:number; uuid:string; title:string; authors:string[]; identifiers:Record<string,string>;
  hierarchy:Hierarchy; publisher:string|null; has_comments:boolean; formats:string[] };
type Reference = { id:string; title:string; authors:string[]; aliases:string[]; identifiers:Record<string,string>;
  book_uuids:string[]; hierarchy:Hierarchy; source_id:string; evidence:string; approved:boolean;
  origin:string; coverage:string; before:string[]; fixed:boolean; edition:string };
type Source = { id:string; name:string; location:string; sha256:string; kind:string;
  segments:{ordinal:number;kind:string;text:string}[] };
type Proposal = { book:Book; status:string; reason:string; reference_id:string|null;
  after:Hierarchy|null; changed:number[]; blocked:boolean; pinned:boolean[] };
type Operation = { id:string; created:number; library:string; phase:string; kind:string;
  backup:string|null; error:string|null; changes:unknown[]; verification:unknown };
type Settings = { library:string; columns:[string,string,string]; calibre_binary:string };
type Alias = { id:string; existing_series:string; universe:string };
type Snapshot = { workspace:string; data:{settings:Settings; references:Reference[]; sources:Source[];aliases:Alias[]};
  proposals:Proposal[]; history:Operation[]; issues:Record<string,string> };

let state:Snapshot;
let tab="Library", busy=false, query="", status="", page=0;
const selected=new Map<number,Set<number>>();
const root=document.querySelector<HTMLDivElement>("#app")!;
const heading=document.createElement("header"), nav=document.createElement("nav"), content=document.createElement("main");
const notice=document.createElement("div"); notice.className="notice"; notice.setAttribute("role","status");
root.append(heading,nav,notice,content);
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
  nav.replaceChildren(...["Library","References & sources","Review","History","Diagnostics","Settings"].map(name=>
    button(name,()=>{tab=name;render();},tab===name?"active":"")));
  content.replaceChildren();
  if(!state)return;
  if(tab==="Library")audit();
  else if(tab==="References & sources")references();
  else if(tab==="Review")review();
  else if(tab==="History")history();
  else if(tab==="Diagnostics")diagnostics();
  else settings();
}
function title(text:string,description:string) {content.append(el("h1",text),el("p",description,"muted"));}
function audit() {
  title("Library audit","Existing series values are not automatically certified. Protected numbering is retained; unresolved works require a reviewed reference.");
  const counts=state.proposals.reduce<Record<string,number>>((a,p)=>{a[p.status]=(a[p.status]??0)+1;return a;},{});
  const stats=el("div","","stats");
  stats.append(el("div",`${state.proposals.length} books`));
  for(const [key,count] of Object.entries(counts))stats.append(el("div",`${count} ${key}`,key));
  content.append(stats);
  const tools=el("div","","toolbar"),search=field("Search title, author or series",query);
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
      && [p.book.title,...p.book.authors,...p.book.hierarchy.map(v=>v.name??"")].join(" ").toLowerCase().includes(needle));
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
    el("p","Empty target fields clear existing values. Verify whole-book edition scope and independent subseries numbering before approving.","warning"));
  const work=field("Work title",r.title),authors=field("Authors (separate with |)",r.authors.join(" | ")),
    aliases=field("Approved title aliases (|)",r.aliases.join(" | ")),uuids=field("Explicit bound UUIDs (|)",r.book_uuids.join(" | "));
  box.append(work.wrapper,authors.wrapper,aliases.wrapper,uuids.wrapper);
  if(r.id)box.append(el("small",`Rule ID: ${r.id}`));
  const values=r.hierarchy.map((v,i)=>{
    const group=el("div","","toolbar"),name=field(i===0?"Series: overarching universe":`Series${i+1}: genuine subseries`,v.name??""),
      number=field("Independent number",v.index?.toString()??"","number");
    number.input.step="any";group.append(name.wrapper,number.wrapper);box.append(group);return {name,number};
  });
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
    card.append(el("h2",`${op.kind} · ${op.phase} · ${op.changes.length} books`),
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

async function start() {
  try {
    state=await invoke<Snapshot>("dispatch",{action:"snapshot",payload:{}});
    render();
    await act("scan",{},"Read-only scan complete. No Calibre metadata was changed.");
  } catch(error) {if(state)render();message(String(error),true);}
}
void start();
