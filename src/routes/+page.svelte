<script lang="ts">
  import { onMount } from "svelte";
  import { listen } from "@tauri-apps/api/event";
  import { invoke } from "@tauri-apps/api/core";

  let log = "";
  let isConnected = false;
  let logContainer: HTMLDivElement;
  let csvData: string[][] = [];
  let csvHeaders: string[] = [];
  let isLoadingCsv = false;
  let csvError = "";
  let networkData = {
    osc_listen_port: "_",
    osc_send_port: "_",
    osc_send_host: "_",
  };

  onMount(() => {
    console.log("Component mounted, setting up listeners...");

    // Listen for backend-log events FIRST
    const backendLogPromise = listen<string>("backend-log", (event) => {
      console.log("Received backend-log event:", event.payload);
      log += event.payload + "\n";
      // Scroll to bottom after DOM updates
      setTimeout(() => {
        if (logContainer) {
          logContainer.scrollTop = logContainer.scrollHeight;
        }
      });
    });

    const networkDataPromise = listen<string>("network-data", (event) => {
      console.log("Received network-data event:", event.payload);
      try {
        const payload = JSON.parse(event.payload);
        const {
          osc_listen_port = "_",
          osc_send_port = "_",
          osc_send_host = "_",
        } = payload || {};

        networkData = {
          osc_listen_port,
          osc_send_port,
          osc_send_host,
        };
      } catch (error) {
        console.error("Failed to load network data:", error);
      }
    });

    // Then start the backend process
    console.log("Calling run_backend...");
    invoke("run_backend")
      .then(() => {
        console.log("run_backend invoke completed");
        isConnected = true;
        // Load CSV after backend starts
        setTimeout(() => loadCsvFile(), 2000); // Wait a bit for backend to create file
      })
      .catch((error) => {
        console.error("run_backend invoke failed:", error);
        log += `Error: ${error}
`;
        isConnected = false;
      });

    // Clean up listener on unmount
    return () => {
      backendLogPromise.then((unlisten) => unlisten());
      networkDataPromise.then((unlisten) => unlisten());
      isConnected = false;
    };
  });

  function clearLog() {
    log = "";
  }

  function parseCsv(csvText: string): { headers: string[]; data: string[][] } {
    const lines = csvText.trim().split("\n");
    if (lines.length === 0) return { headers: [], data: [] };

    const headers = lines[0].split(",").map((h) => h.trim().replace(/"/g, ""));
    const data = lines
      .slice(1)
      .map((line) =>
        line.split(",").map((cell) => cell.trim().replace(/"/g, ""))
      );

    return { headers, data };
  }

  async function loadCsvFile() {
    isLoadingCsv = true;
    csvError = "";

    try {
      console.log("Attempting to load CSV...");

      const csvContent = await invoke<string>("read_csv_file");
      const parsed = parseCsv(csvContent);

      csvHeaders = parsed.headers;
      csvData = parsed.data;

      console.log("CSV loaded successfully:", {
        headers: csvHeaders,
        rows: csvData.length,
      });
    } catch (error) {
      console.error("Failed to load CSV:", error);
      csvError = `Failed to load CSV: ${error}`;
    } finally {
      isLoadingCsv = false;
    }
  }
</script>

<main class="container">
  <header class="header">
    <div class="header-left">
      <h1 class="title">oDIsc</h1>
      <div class="osc-info">
        <span>Incoming Port: {networkData.osc_listen_port}</span>
        <span
          >Outgoing Address: {networkData.osc_send_host}:{networkData.osc_send_port}</span
        >
      </div>
    </div>
    <div class="header-controls">
      <div class="status">
        <div class="status-indicator" class:connected={isConnected}></div>
        <span class="status-text">
          {isConnected ? "Connected" : "Disconnected"}
        </span>
      </div>
      <button
        class="load-csv-btn"
        on:click={loadCsvFile}
        disabled={isLoadingCsv}
      >
        {#if isLoadingCsv}
          Loading...
        {:else}
          Refresh Mappings
        {/if}
      </button>
    </div>
  </header>

  <!-- Console Logs Section -->
  <div class="log-section">
    <div class="log-header">
      <h2>Backend Logs</h2>
      <button class="clear-btn" on:click={clearLog} disabled={!log}>
        Clear
      </button>
    </div>

    <div class="log-container" bind:this={logContainer}>
      {#if log}
        <pre class="log-content">{log}</pre>
      {:else}
        <div class="empty-state">
          <div class="empty-icon">📋</div>
          <p>No logs yet. Waiting for backend output...</p>
        </div>
      {/if}
    </div>
  </div>

  <!-- CSV Table Section - Separate card underneath -->
  <div class="table-section">
    <div class="table-header">
      <h2>
        {#if csvData.length > 0}
          Mappings ({csvData.length} rows)
        {:else if isLoadingCsv}
          Loading Mappings...
        {:else if csvError}
          Mappings (Error)
        {:else}
          Mappings
        {/if}
      </h2>
      <button
        class="refresh-btn"
        on:click={loadCsvFile}
        disabled={isLoadingCsv}
      >
        🔄 Refresh
      </button>
    </div>

    <div class="table-container">
      {#if csvError}
        <div class="error-message">
          <div class="error-icon">⚠️</div>
          <p>{csvError}</p>
          <button class="retry-btn" on:click={loadCsvFile}>Try Again</button>
        </div>
      {:else if isLoadingCsv}
        <div class="loading-state">
          <div class="loading-icon">⏳</div>
          <p>Loading CSV file...</p>
        </div>
      {:else if csvData.length > 0}
        <table class="csv-table">
          <thead>
            <tr>
              {#each csvHeaders as header}
                <th>{header}</th>
              {/each}
            </tr>
          </thead>
          <tbody>
            {#each csvData as row}
              <tr>
                {#each row as cell}
                  <td>{cell}</td>
                {/each}
              </tr>
            {/each}
          </tbody>
        </table>
      {:else}
        <div class="empty-table-state">
          <div class="empty-icon">📊</div>
          <p>No mappings data available</p>
          <p class="empty-subtitle">
            The CSV file may not exist yet or be empty
          </p>
        </div>
      {/if}
    </div>
  </div>
</main>

<style>
  :global(body) {
    margin: 0;
    padding: 0;
    box-sizing: border-box;
  }

  .container {
    min-height: 100vh;
    background: linear-gradient(135deg, #1e293b 0%, #0f172a 100%);
    font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto,
      sans-serif;
  }

  .header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    margin-bottom: 2rem;
    background: rgba(30, 41, 59, 0.8);
    backdrop-filter: blur(10px);
    border-radius: 16px;
    padding: 1.5rem 2rem;
    border: 1px solid rgba(71, 85, 105, 0.3);
  }

  .header-left {
    display: flex;
    align-items: baseline;
    gap: 1.5rem;
  }

  .title {
    margin: 0;
    font-size: 2.5rem;
    font-weight: 700;
    color: #f1f5f9;
    text-shadow: 0 2px 4px rgba(0, 0, 0, 0.5);
  }

  .osc-info {
    display: flex;
    gap: 1rem;
    font-size: 0.875rem;
    color: #94a3b8;
    font-family: "SF Mono", Monaco, "Cascadia Code", "Roboto Mono", Consolas,
      "Courier New", monospace;
  }

  .osc-info span:not(:last-child)::after {
    content: '|';
    color: #475569;
    margin-left: 1rem;
  }

  .header-controls {
    display: flex;
    align-items: center;
    gap: 1rem;
  }

  .status {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    background: rgba(51, 65, 85, 0.6);
    padding: 0.5rem 1rem;
    border-radius: 20px;
    border: 1px solid rgba(71, 85, 105, 0.4);
  }

  .status-indicator {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: #ef4444;
    transition: background-color 0.3s ease;
  }

  .status-indicator.connected {
    background: #22c55e;
  }

  .status-text {
    color: #e2e8f0;
    font-size: 0.875rem;
    font-weight: 500;
  }

  .load-csv-btn {
    background: #3b82f6;
    color: white;
    border: none;
    padding: 0.5rem 1rem;
    border-radius: 8px;
    font-size: 0.875rem;
    font-weight: 500;
    cursor: pointer;
    transition: all 0.2s ease;
  }

  .load-csv-btn:hover:not(:disabled) {
    background: #2563eb;
    transform: translateY(-1px);
  }

  .load-csv-btn:disabled {
    background: #475569;
    cursor: not-allowed;
    transform: none;
  }

  .log-section,
  .table-section {
    background: rgba(15, 23, 42, 0.95);
    border-radius: 16px;
    overflow: hidden;
    box-shadow:
      0 20px 25px -5px rgba(0, 0, 0, 0.3),
      0 10px 10px -5px rgba(0, 0, 0, 0.2);
    border: 1px solid rgba(71, 85, 105, 0.3);
    margin-bottom: 2rem;
  }

  .table-section {
    margin-bottom: 0;
  }

  .log-header,
  .table-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 1.5rem 2rem;
    background: rgba(30, 41, 59, 0.6);
    border-bottom: 1px solid rgba(71, 85, 105, 0.3);
  }

  .log-header h2,
  .table-header h2 {
    margin: 0;
    font-size: 1.25rem;
    font-weight: 600;
    color: #f1f5f9;
  }

  .clear-btn,
  .refresh-btn {
    background: #dc2626;
    color: white;
    border: none;
    padding: 0.5rem 1rem;
    border-radius: 8px;
    font-size: 0.875rem;
    font-weight: 500;
    cursor: pointer;
    transition: all 0.2s ease;
  }

  .refresh-btn {
    background: #059669;
  }

  .clear-btn:hover:not(:disabled) {
    background: #b91c1c;
    transform: translateY(-1px);
  }

  .refresh-btn:hover:not(:disabled) {
    background: #047857;
    transform: translateY(-1px);
  }

  .clear-btn:disabled,
  .refresh-btn:disabled {
    background: #475569;
    cursor: not-allowed;
    transform: none;
  }

  .log-container {
    height: 50vh;
    overflow-y: auto;
    padding: 1.5rem 2rem;
    background: #0f172a;
  }

  .table-container {
    height: 50vh;
    overflow: auto;
    background: #0f172a;
  }

  .csv-table {
    width: 100%;
    border-collapse: collapse;
    font-size: 0.875rem;
  }

  .csv-table th {
    background: #1e293b;
    color: #f1f5f9;
    padding: 0.75rem 1rem;
    text-align: left;
    font-weight: 600;
    border-bottom: 2px solid #334155;
    position: sticky;
    top: 0;
    z-index: 1;
  }

  .csv-table td {
    padding: 0.75rem 1rem;
    border-bottom: 1px solid #334155;
    color: #e2e8f0;
  }

  .csv-table tr:hover {
    background: rgba(30, 41, 59, 0.3);
  }

  .log-content {
    color: #e2e8f0;
    font-family: "SF Mono", Monaco, "Cascadia Code", "Roboto Mono", Consolas,
      "Courier New", monospace;
    font-size: 0.875rem;
    line-height: 1.6;
    margin: 0;
    white-space: pre-wrap;
    word-break: break-word;
  }

  .empty-state,
  .error-message,
  .loading-state,
  .empty-table-state {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    height: 100%;
    color: #64748b;
    text-align: center;
  }

  .error-message {
    color: #ef4444;
  }

  .empty-icon,
  .error-icon,
  .loading-icon {
    font-size: 3rem;
    margin-bottom: 1rem;
    opacity: 0.5;
  }

  .empty-state p,
  .error-message p,
  .loading-state p,
  .empty-table-state p {
    margin: 0;
    font-size: 1rem;
  }

  .empty-subtitle {
    font-size: 0.875rem !important;
    opacity: 0.7;
    margin-top: 0.5rem !important;
  }

  .retry-btn {
    background: #3b82f6;
    color: white;
    border: none;
    padding: 0.5rem 1rem;
    border-radius: 8px;
    font-size: 0.875rem;
    font-weight: 500;
    cursor: pointer;
    margin-top: 1rem;
    transition: all 0.2s ease;
  }

  .retry-btn:hover {
    background: #2563eb;
  }

  /* Custom scrollbar */
  .log-container::-webkit-scrollbar,
  .table-container::-webkit-scrollbar {
    width: 8px;
    height: 8px;
  }

  .log-container::-webkit-scrollbar-track,
  .table-container::-webkit-scrollbar-track {
    background: #1e293b;
  }

  .log-container::-webkit-scrollbar-thumb,
  .table-container::-webkit-scrollbar-thumb {
    background: #475569;
    border-radius: 4px;
  }

  .log-container::-webkit-scrollbar-thumb:hover,
  .table-container::-webkit-scrollbar-thumb:hover {
    background: #64748b;
  }
</style>