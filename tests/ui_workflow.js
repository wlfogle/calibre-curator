// Fictional IPC fixtures only. The real backend is tested separately on disposable Calibre libraries.
const blank = () => ({name:null,index:null});
const book = {id:1,uuid:"fixture",title:"Owned Work",authors:["Fixture Author"],identifiers:{},
  publisher:null,has_comments:false,formats:["EPUB"],hierarchy:[{name:null,index:1},blank(),blank(),blank()]};
const reference = {id:"rule",title:book.title,authors:book.authors,identifiers:{},book_uuids:[],aliases:[],
  hierarchy:[{name:"Fixture Universe",index:2},{name:"Island",index:1},blank(),blank()],source_id:"source",
  evidence:"Fixture ordered source",approved:true,origin:"verified source document",coverage:"partial",
  before:[],fixed:true,edition:"any",field_mask:[true,true,false,false]};
const snapshot = {workspace:"/tmp/fictional-workspace",issues:{},history:[],proposals:[{book,status:"unresolved",
  reason:"Source not verified",reference_id:null,after:null,changed:[],blocked:true,pinned:[false,false,false,false]}],
  data:{settings:{library:"/tmp/fictional-library",columns:["series2","series3","series4"],calibre_binary:"/usr/bin/calibre-debug"},
    references:[],sources:[{id:"source",name:"Fictional reference",kind:"html",location:"fixture",sha256:"fixture",
      segments:[{ordinal:1,kind:"p",text:"Missing Work (2000) – Island #0"},{ordinal:2,kind:"p",text:"Owned Work (2001) – Island #1"}]}],
    source_reviews:{},research_runs:[],chronology_runs:[],repair_notes:[],aliases:[],ai_settings:{enabled:false},ai_models:[],ai_runs:[],ai_decisions:{},
    metadata_settings:{search_all:true,providers:[],fetch_binary:"/usr/bin/fetch-ebook-metadata",timeout_seconds:15,cache_days:7},
    metadata_providers:[],synopsis_drafts:[],metadata_results:[{id:"lookup",library:"/tmp/fictional-library",book_id:1,book_uuid:"fixture",
      provider:{name:"Fixture",version:"1",identify:true,enabled:true},fetched_at:1,cached:true,match_status:"compatible",
      note:"Fixture work/edition match",identity_confirmed:false,source_id:"metadata-source",
      metadata:{title:book.title,authors:book.authors,publisher:"Fixture Press",identifiers:{goodreads:"1234"},url:null,
        description:"A botanist investigates dying gardens on an isolated island. She works with a local gardener to trace the damage to a blocked stream while a storm threatens the village."}}]}};
window.__UI_CALLS__=[];
window.__TAURI_INTERNALS__={
  transformCallback:()=>1,
  invoke:async(command,args)=>{
    if(command==="plugin:event|listen")return 1;
    if(command==="preview_source")return {entries:[{reference,status:"matched",matches:1}],skipped:[]};
    if(command!=="dispatch")throw new Error(`Unexpected IPC ${command}`);
    window.__UI_CALLS__.push(JSON.parse(JSON.stringify(args)));
    if(args.action==="source_verify"){
      snapshot.data.source_reviews.source=args.payload.review;snapshot.data.references=[reference];
      Object.assign(snapshot.proposals[0],{status:"missing",reference_id:"rule",after:reference.hierarchy,changed:[0,1],blocked:false});
    } else if(args.action==="chronology_research") {
      snapshot.data.chronology_runs=[{id:"chronology",book_id:1,book_uuid:book.uuid,library:snapshot.data.settings.library,
        created:1,cached:false,status:"ready",hierarchy:reference.hierarchy,field_mask:[true,true,false,false],
        notes:["Fictional source chronology"],queries:["fixture query"],model:"",digest:"",anchors:[],evidence:[]}];
    } else if(args.action==="repair_apply") {
      snapshot.history=[{id:"repair",created:1,library:snapshot.data.settings.library,metadata_field:"repair",
        kind:"apply",phase:"verified",changes:[{}],backup:"/tmp/fictional-backup",backup_coverage:"full-library",backup_verified:true,error:null}];
    } else if(!["snapshot","scan","reverse"].includes(args.action))throw new Error(`Unexpected action ${args.action}`);
    return JSON.parse(JSON.stringify(snapshot));
  }
};
window.__RUN_UI_TEST__=async()=>{
  const assert=(value,message)=>{if(!value)throw new Error(message);};
  const until=async(fn)=>{for(let i=0;i<100;i++){if(fn())return;await new Promise(r=>setTimeout(r,20));}throw new Error("UI condition timed out");};
  const button=text=>[...document.querySelectorAll("button")].find(b=>b.textContent===text);
  const click=async text=>{const b=button(text);assert(b&&!b.disabled,`Missing/enabled button: ${text}`);b.click();await new Promise(r=>setTimeout(r,30));};
  const input=label=>[...document.querySelectorAll("label")].find(l=>l.textContent.includes(label))?.querySelector("input");
  const set=(label,value)=>{const el=input(label);assert(el,`Missing input ${label}`);el.value=value;el.dispatchEvent(new Event("input"));};
  await until(()=>button("1. Scan library")&&!button("1. Scan library").disabled);
  assert(document.querySelectorAll("nav button").length===4,"Normal navigation must remain simple");
  await click("Sources");await click("Verify source");
  set("Main series / universe","Fixture Universe");set("Author context","Fixture Author");
  await click("Preview mappings");
  const verify="Verify this source and use its mappings";
  assert(!button(verify).disabled,"Preview must enable source-level verification");
  set("Main series / universe","Changed");
  assert(button(verify).disabled,"Changing settings must invalidate preview");
  set("Main series / universe","Fixture Universe");
  await click("Preview mappings");await click(verify);await click("Repair");
  assert(document.body.textContent.includes("1 supported series corrections"),"Verified source must feed Repair");
  await click("Research chronology and subseries");await click("Confirm");
  assert(input("Include supported research corrections")?.checked,"Research findings must enter the same Repair preview");
  const publisher=input("Fill publisher:");
  assert(publisher?.checked,"Consistent compatible metadata must be prepared without per-book approval");
  publisher.click();
  set("Filter books","Owned");
  assert(!input("Fill publisher:").checked,"Filtering must preserve unchecked fields");
  set("Filter books","");
  await click("Apply selected fixes");
  assert(document.querySelector("dialog[open]"),"Apply must require one batch confirmation");
  await click("Confirm");
  const request=window.__UI_CALLS__.find(c=>c.action==="repair_apply")?.payload;
  assert(request?.research[0]==="chronology"&&request.series.length===0&&request.fills.length===1,"One batch must combine research and missing metadata without duplicate series requests");
  assert(request.fills[0].publisher===false&&request.fills[0].synopsis===true,"Only selected fields may be sent");
  await click("History");
  assert(document.body.textContent.includes("Verified complete library backup, including ebooks"),"History must report full backup coverage");
  await click("Revert this batch");await click("Confirm");
  assert(window.__UI_CALLS__.some(c=>c.action==="reverse"),"Revert must invoke guarded restoration");
  return {status:"passed",checks:["simple navigation","source verification preview","changed-preview invalidation",
    "automatic consistent-provider preparation","field-choice persistence","research-to-batch repair","verified full-backup display","guarded Revert control"]};
};
