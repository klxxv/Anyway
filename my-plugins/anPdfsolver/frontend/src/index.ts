import AnPdfsolverDialog from "./components/AnPdfsolverDialog.vue";
import AnPdfsolverToolbarButton from "./components/AnPdfsolverToolbarButton.vue";
import { clearPluginContext, closeTrackedWorkers, setPluginContext, type PluginContext } from "./context";
import styles from "./styles.css?inline";

export { AnPdfsolverDialog, AnPdfsolverToolbarButton };

export async function activate({ context }: { context: PluginContext }): Promise<void> {
  if (!document.getElementById("anyway-plugin-myc-pdf-canvas-agent-0-5-0")) {
    const style = document.createElement("style");
    style.id = "anyway-plugin-myc-pdf-canvas-agent-0-5-0";
    style.textContent = styles;
    document.head.appendChild(style);
  }
  setPluginContext(context);
  context.logger?.info("anPdfsolver activated", {
    pluginId: context.plugin.id,
    version: context.plugin.version
  });
}

export async function deactivate(): Promise<void> {
  await closeTrackedWorkers();
  clearPluginContext();
}
