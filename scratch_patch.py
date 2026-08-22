import sys

content = open('src/main.ts', encoding='utf-8').read()

old_marketplace = '''        const btn = card.querySelector<HTMLButtonElement>(".btn-install-ext");
        if (btn) {
          btn.addEventListener("click", async () => {'''

new_marketplace = '''        card.addEventListener("click", (e) => {
          if ((e.target as HTMLElement).tagName === "BUTTON") return;
          openExtensionDetail(ext, false);
        });

        const btn = card.querySelector<HTMLButtonElement>(".btn-install-ext");
        if (btn) {
          btn.addEventListener("click", async (e) => {
            e.stopPropagation();'''

content = content.replace(old_marketplace, new_marketplace)

old_installed = '''        card.innerHTML = `
          <div class="openvsx-ext-header">
            <span class="openvsx-ext-title">${ext.name}</span>
            <span class="openvsx-ext-id">v${ext.version}</span>
          </div>
          <div class="openvsx-ext-desc">${ext.description}</div>
          <div class="openvsx-ext-footer">
            <span style="font-size: 10px; color: #00ff80;">● 有効 (Active)</span>
          </div>
        `;
        installedList.appendChild(card);'''

new_installed = '''        card.innerHTML = `
          <div class="openvsx-ext-header">
            <span class="openvsx-ext-title">${ext.name}</span>
            <span class="openvsx-ext-id">v${ext.version}</span>
          </div>
          <div class="openvsx-ext-desc">${ext.description}</div>
          <div class="openvsx-ext-footer">
            <span style="font-size: 10px; color: #00ff80;">● 有効 (Active)</span>
          </div>
        `;
        card.addEventListener("click", () => {
          const fakeOpenVsxExt: OpenVsxExtension = {
            namespace: ext.id.split('.')[0] || '',
            name: ext.name,
            version: ext.version,
            display_name: ext.name,
            description: ext.description,
            download_count: null,
            icon_url: null,
            download_url: null,
            url: null
          };
          openExtensionDetail(fakeOpenVsxExt, true);
        });
        installedList.appendChild(card);'''

content = content.replace(old_installed, new_installed)

modal_logic = '''
function openExtensionDetail(ext: OpenVsxExtension, isInstalled: boolean) {
  const modal = document.getElementById("ext-detail-modal");
  if (!modal) return;
  
  const icon = document.getElementById("ext-detail-icon") as HTMLImageElement;
  const title = document.getElementById("ext-detail-title");
  const id = document.getElementById("ext-detail-id");
  const desc = document.getElementById("ext-detail-desc");
  const readme = document.getElementById("ext-detail-readme");
  
  const installBtn = document.getElementById("ext-detail-install-btn") as HTMLButtonElement;
  const uninstallBtn = document.getElementById("ext-detail-uninstall-btn") as HTMLButtonElement;
  const closeBtn = document.getElementById("ext-detail-close") as HTMLButtonElement;
  
  icon.src = ext.icon_url || "https://via.placeholder.com/72?text=Ext";
  if (title) title.textContent = ext.display_name || ext.name;
  if (id) id.textContent = `${ext.namespace}.${ext.name} v${ext.version}`;
  if (desc) desc.textContent = ext.description || "";
  if (readme) readme.innerHTML = "Fetching README...";
  
  if (isInstalled) {
    installBtn.classList.add("hidden");
    uninstallBtn.classList.remove("hidden");
  } else {
    installBtn.classList.remove("hidden");
    uninstallBtn.classList.add("hidden");
    installBtn.textContent = "インストール";
    installBtn.disabled = false;
  }
  
  installBtn.onclick = async () => {
    installBtn.textContent = "インストール中...";
    installBtn.disabled = true;
    try {
      const res = await invoke<string>("install_openvsx_extension", {
        namespace: ext.namespace,
        name: ext.name,
        version: ext.version,
        description: ext.description || "",
        downloadUrl: ext.download_url || null,
      });
      showStatusMessage(res);
      installBtn.textContent = "✓ インストール済み";
      uninstallBtn.classList.remove("hidden");
      installBtn.classList.add("hidden");
      
      if (ext.name.includes("rust")) ensureLspServerStarted("rust");
      if (ext.name.includes("python")) ensureLspServerStarted("python");
      if (ext.name.includes("go")) ensureLspServerStarted("go");
    } catch (err) {
      alert(`エラー: ${err}`);
      installBtn.textContent = "インストール";
      installBtn.disabled = false;
    }
  };
  
  uninstallBtn.onclick = async () => {
    uninstallBtn.textContent = "アンインストール中...";
    uninstallBtn.disabled = true;
    try {
      const res = await invoke<string>("uninstall_extension", {
        id: `${ext.namespace}.${ext.name}`
      });
      showStatusMessage(res);
      installBtn.classList.remove("hidden");
      uninstallBtn.classList.add("hidden");
    } catch (err) {
      alert(`アンインストール失敗: ${err}`);
      uninstallBtn.textContent = "アンインストール";
      uninstallBtn.disabled = false;
    }
  };
  
  closeBtn.onclick = () => {
    modal.classList.add("hidden");
  };
  
  modal.classList.remove("hidden");
  
  if (ext.url) {
    fetch(ext.url).then(r => r.json()).then(data => {
      if (readme) {
        readme.innerHTML = `<div style="padding: 10px;">
          <h3>${ext.display_name || ext.name}</h3>
          <p>${ext.description || ''}</p>
          <hr>
          <p>Repository: ${data.repository || 'N/A'}</p>
          <p>License: ${data.license || 'N/A'}</p>
          <p>Downloads: ${ext.download_count}</p>
        </div>`;
      }
    }).catch(() => {
      if (readme) readme.innerHTML = "Failed to load details.";
    });
  } else {
    if (readme) readme.innerHTML = "No additional details available.";
  }
}
'''

content += '\n' + modal_logic
open('src/main.ts', 'w', encoding='utf-8').write(content)
print('Patched successfully')
