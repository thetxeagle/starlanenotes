// Purpose-private conservative facts. No accounting subtraction or live authority
// is created by this report, and existing physical UsageReport stays unchanged.
#[derive(Clone,Debug,PartialEq,Eq)]
pub(crate) struct HistoricalUsageFacts {
 pub immutable_logical_charge_bytes:i64,
 pub pending_maximum_bytes:i64,
 pub conservative_total_bytes:i64,
 pub reference_semantics:historical::HistoricalReferenceSemantics,
}
#[derive(Clone,Copy)]enum HistoricalInspectionPurpose{Physical,ConservativeUsage}
struct HistoricalInspectedFacts{physical:HistoricalPhysicalSummary,conservative_usage:Option<HistoricalUsageFacts>}
fn historical_checked_usage(logical:i64,pending:i64)->io::Result<HistoricalUsageFacts>{
 if logical<0||pending<0{return Err(fail("Historical conservative negative charge refused"));}
 let total=logical.checked_add(pending).ok_or_else(||fail("Historical conservative charge overflow"))?;
 Ok(HistoricalUsageFacts{immutable_logical_charge_bytes:logical,pending_maximum_bytes:pending,conservative_total_bytes:total,reference_semantics:historical::HistoricalReferenceSemantics::NotEstablished})
}
struct HistoricalUsageObservation<'a,'b>{physical:&'a HistoricalFinalizedObservation<'b>,facts:HistoricalUsageFacts}
impl<'a,'b> HistoricalUsageObservation<'a,'b>{
 async fn sums_in(physical:&HistoricalFinalizedObservation<'_>,conn:&mut sqlx::AnyConnection)->io::Result<HistoricalUsageFacts>{
  use sqlx::Row;physical.recheck_in(conn).await?;
  // Existing logical-only consistency validates all immutable registry/receipt
  // attribution without copied native WriterLease or historical subtraction.
  let logical=crate::storage::attachment_capacity::verify_retained_logical_in(conn).await.map_err(|_|fail("Historical complete immutable logical attribution unavailable"))?;
  let rows=sqlx::query("SELECT operation_id,maximum_bytes,state FROM attachment_capacity_reservations ORDER BY operation_id LIMIT 513").fetch_all(&mut *conn).await.map_err(|_|fail("Historical conservative complete reservations unavailable"))?;
  if rows.len()>512{return Err(fail("Historical conservative reservation bound exceeded"));}
  let mut pending=0i64;
  for row in rows{
   let maximum:i64=row.try_get("maximum_bytes").map_err(|_|fail("Historical original maximum unavailable"))?;
   if maximum<0{return Err(fail("Historical original maximum negative"));}
   let state:String=row.try_get("state").map_err(|_|fail("Historical original reservation state unavailable"))?;
   match state.as_str(){"reserved"=>pending=pending.checked_add(maximum).ok_or_else(||fail("Historical reserved maxima overflow"))?,"committed"|"disposed"=>{},_=>return Err(fail("Historical conservative unknown reservation state")),}
  }
  if u64::try_from(pending).ok()!=Some(physical.summary()?.retained_pending_maximum){return Err(fail("Historical exact original pending maximum differs"));}
  let facts=historical_checked_usage(logical,pending)?;physical.recheck_in(conn).await?;Ok(facts)
 }
 async fn hold_in(physical:&'a HistoricalFinalizedObservation<'b>,conn:&mut sqlx::AnyConnection)->io::Result<Self>{let facts=Self::sums_in(physical,conn).await?;Ok(Self{physical,facts})}
 async fn recheck_in(&self,conn:&mut sqlx::AnyConnection)->io::Result<()>{if Self::sums_in(self.physical,conn).await?!=self.facts{return Err(fail("Historical held conservative accounting changed"));}Ok(())}
}
/// Returns bounded conservative facts only after releasing actual exclusion.
/// Never use this value as a live quota/cleanup or application read grant.
pub(crate) async fn inspect_finalized_historical_sqlite_usage(path:&Path,writers_stopped:bool)->io::Result<HistoricalUsageFacts>{
 #[cfg(test)]let facts=inspect_finalized_historical_sqlite_inner(path,writers_stopped,HistoricalInspectionPurpose::ConservativeUsage,None).await?;
 #[cfg(not(test))]let facts=inspect_finalized_historical_sqlite_inner(path,writers_stopped,HistoricalInspectionPurpose::ConservativeUsage).await?;
 facts.conservative_usage.ok_or_else(||fail("Historical held usage observation absent"))
}
#[cfg(test)]pub(crate) async fn inspect_finalized_historical_foreign_usage_for_test(path:&Path,foreign:&Path)->io::Result<HistoricalUsageFacts>{inspect_finalized_historical_sqlite_inner(path,true,HistoricalInspectionPurpose::ConservativeUsage,Some(foreign)).await?.conservative_usage.ok_or_else(||fail("Historical owned foreign test usage absent"))}
#[cfg(test)]fn historical_usage_foreign_writer_blocked(path:&Path)->io::Result<()>{
 let code=r#"import sqlite3,sys
c=sqlite3.connect(sys.argv[1],timeout=0.05)
try:
 c.execute('BEGIN IMMEDIATE')
 c.execute('UPDATE _sqlx_migrations SET execution_time=execution_time+1 WHERE version=1')
 c.commit()
 sys.exit(42)
except sqlite3.OperationalError as e:
 if 'locked' not in str(e).lower():raise
 c.rollback()
 sys.exit(10)
"#;
 let child=std::process::Command::new("/usr/bin/python3").args(["-I","-c",code]).arg(path.join("starlane-notes.db")).output()?;
 if child.status.code()!=Some(10){return Err(fail("Historical held usage failed actual independent writer exclusion"));}
 println!("ACTUAL_HISTORICAL_USAGE_ORIGINAL_RESERVATION_FOREIGN_PROCESS_BLOCKED_PASS");Ok(())
}
#[cfg(test)]mod historical_usage_arithmetic_tests{
 use super::*;
 #[test]fn conservative_totals_do_not_wrap_or_subtract(){assert!(historical_checked_usage(i64::MAX,1).is_err());assert!(historical_checked_usage(-1,1).is_err());assert!(historical_checked_usage(1,-1).is_err());let v=historical_checked_usage(29,1048576).unwrap();assert_eq!(v.conservative_total_bytes,1048605);assert_eq!(v.immutable_logical_charge_bytes,29);assert_eq!(v.pending_maximum_bytes,1048576);}
}
