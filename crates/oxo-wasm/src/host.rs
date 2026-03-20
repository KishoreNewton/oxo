//! WASM host functions and runtime management.
//!
//! Provides the execution environment for WASM plugins, including memory
//! management for passing data between the host and guest.

use anyhow::Result;
use wasmtime::*;

use oxo_core::backend::LogEntry;

/// The WASM runtime environment for executing plugins.
pub struct WasmHost {
    engine: Engine,
}

impl WasmHost {
    /// Create a new WASM host with default settings.
    pub fn new() -> Result<Self> {
        let mut config = Config::new();
        config.wasm_bulk_memory(true);
        config.cranelift_opt_level(OptLevel::Speed);
        // Fuel-based metering to prevent infinite loops.
        config.consume_fuel(true);

        let engine = Engine::new(&config)?;
        Ok(Self { engine })
    }

    /// Execute a transform plugin on a batch of log entries.
    ///
    /// The plugin's `transform` function receives JSON-serialized entries
    /// and returns transformed entries as JSON.
    pub fn run_transform(&self, wasm_bytes: &[u8], entries: &[LogEntry]) -> Result<Vec<LogEntry>> {
        let module = Module::new(&self.engine, wasm_bytes)?;
        let mut store = Store::new(&self.engine, ());

        // Give the plugin a generous fuel budget.
        store.set_fuel(1_000_000)?;

        let linker = Linker::new(&self.engine);
        let instance = linker.instantiate(&mut store, &module)?;

        // Get the plugin's memory and exported functions.
        let memory = instance
            .get_memory(&mut store, "memory")
            .ok_or_else(|| anyhow::anyhow!("plugin has no exported memory"))?;

        let alloc_fn = instance
            .get_typed_func::<i32, i32>(&mut store, "alloc")
            .ok();

        let transform_fn = instance.get_typed_func::<(i32, i32), i64>(&mut store, "transform")?;

        // Serialize input entries to JSON.
        let input_json = serde_json::to_vec(entries)?;
        let input_len = input_json.len() as i32;

        // Allocate memory in the guest for the input.
        let input_ptr = if let Some(ref alloc) = alloc_fn {
            alloc.call(&mut store, input_len)?
        } else {
            // If no alloc function, write to a fixed offset.
            0
        };

        // Write input data to guest memory.
        let start = input_ptr as usize;
        let end = start + input_len as usize;
        {
            let mem_data = memory.data(&store);
            if end > mem_data.len() {
                let _ = mem_data;
                memory.grow(&mut store, ((end / 65536) + 1) as u64)?;
            }
        }
        let mem_data = memory.data_mut(&mut store);
        mem_data[start..end].copy_from_slice(&input_json);

        // Call the transform function.
        let result = transform_fn.call(&mut store, (input_ptr, input_len))?;

        // Decode the result: high 32 bits = pointer, low 32 bits = length.
        let result_ptr = (result >> 32) as usize;
        let result_len = (result & 0xFFFFFFFF) as usize;

        if result_len == 0 {
            // Plugin returned empty → pass through unchanged.
            return Ok(entries.to_vec());
        }

        // Read result from guest memory.
        let mem_data = memory.data(&store);
        if result_ptr + result_len > mem_data.len() {
            return Ok(entries.to_vec());
        }

        let result_slice = &mem_data[result_ptr..result_ptr + result_len];
        let transformed: Vec<LogEntry> = serde_json::from_slice(result_slice)?;

        Ok(transformed)
    }

    /// Execute a filter plugin on a batch of log entries.
    ///
    /// The plugin's `filter` function receives a JSON-serialized entry
    /// and returns 1 (keep) or 0 (drop).
    pub fn run_filter(&self, wasm_bytes: &[u8], entries: &[LogEntry]) -> Result<Vec<LogEntry>> {
        let module = Module::new(&self.engine, wasm_bytes)?;
        let mut store = Store::new(&self.engine, ());
        store.set_fuel(1_000_000)?;

        let linker = Linker::new(&self.engine);
        let instance = linker.instantiate(&mut store, &module)?;

        let memory = instance
            .get_memory(&mut store, "memory")
            .ok_or_else(|| anyhow::anyhow!("plugin has no exported memory"))?;

        let filter_fn = instance.get_typed_func::<(i32, i32), i32>(&mut store, "filter")?;

        let mut result = Vec::with_capacity(entries.len());

        for entry in entries {
            let entry_json = serde_json::to_vec(entry)?;
            let entry_len = entry_json.len() as i32;

            // Write entry to guest memory at offset 0.
            let end = entry_len as usize;
            {
                let mem_data = memory.data(&store);
                if end > mem_data.len() {
                    let _ = mem_data;
                    memory.grow(&mut store, ((end / 65536) + 1) as u64)?;
                }
            }
            let mem_data = memory.data_mut(&mut store);
            mem_data[..end].copy_from_slice(&entry_json);

            // Call filter: returns 1 to keep, 0 to drop.
            let keep = filter_fn.call(&mut store, (0, entry_len))?;
            if keep != 0 {
                result.push(entry.clone());
            }
        }

        Ok(result)
    }
}

impl Default for WasmHost {
    fn default() -> Self {
        Self::new().expect("failed to create WASM host")
    }
}
