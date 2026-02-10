pub mod collector;
pub mod container;
pub mod output;

use crate::utils::Result;

pub fn run_check(container: Option<String>, output_format: &str, verbose: bool) -> Result<()> {
    println!("Collecting Docker information...\n");
    
    // 收集容器信息
    let containers = if let Some(cid) = container {
        vec![collector::collect_container_info(&cid, verbose)?]
    } else {
        collector::collect_all_containers(verbose)?
    };
    
    // 输出结果
    output::display_containers(&containers, output_format)?;
    
    Ok(())
}