// Tab switching
document.querySelectorAll('.tab-button').forEach(button => {
    button.addEventListener('click', () => {
        // Remove active class from all tabs
        document.querySelectorAll('.tab-button').forEach(b => b.classList.remove('active'));
        document.querySelectorAll('.upload-panel').forEach(p => p.classList.add('hidden'));

        // Add active class to clicked tab
        button.classList.add('active');
        const type = button.getAttribute('data-type');
        document.getElementById(`${type}-panel`).classList.remove('hidden');
    });
});

// Setup drag and drop for each type
['pypi', 'npm', 'cargo'].forEach(type => {
    const zone = document.getElementById(`${type}-zone`);
    const fileInput = document.getElementById(`${type}-file`);
    const statusDiv = document.getElementById(`${type}-status`);

    // Click to browse
    zone.addEventListener('click', () => fileInput.click());

    // File selection
    fileInput.addEventListener('change', (e) => {
        handleFiles(e.target.files, type, statusDiv);
    });

    // Drag events
    zone.addEventListener('dragover', (e) => {
        e.preventDefault();
        zone.classList.add('drag-over');
    });

    zone.addEventListener('dragleave', () => {
        zone.classList.remove('drag-over');
    });

    zone.addEventListener('drop', (e) => {
        e.preventDefault();
        zone.classList.remove('drag-over');
        handleFiles(e.dataTransfer.files, type, statusDiv);
    });
});

// Handle file uploads
async function handleFiles(files, type, statusDiv) {
    statusDiv.innerHTML = '';

    for (const file of files) {
        const fileStatus = document.createElement('div');
        fileStatus.className = 'file-upload-status';

        // Create elements safely to prevent XSS
        const fileName = document.createElement('span');
        fileName.className = 'file-name';
        fileName.textContent = file.name; // Use textContent to prevent XSS

        const fileSize = document.createElement('span');
        fileSize.className = 'file-size';
        fileSize.textContent = formatSize(file.size);

        const status = document.createElement('span');
        status.className = 'status uploading';
        status.textContent = 'Uploading...';

        fileStatus.appendChild(fileName);
        fileStatus.appendChild(fileSize);
        fileStatus.appendChild(status);
        statusDiv.appendChild(fileStatus);

        try {
            await uploadFile(file, type);
            fileStatus.querySelector('.status').textContent = 'Success';
            fileStatus.querySelector('.status').className = 'status success';
        } catch (error) {
            fileStatus.querySelector('.status').textContent = `Failed: ${error.message}`;
            fileStatus.querySelector('.status').className = 'status error';
        }
    }
}

// Upload a single file
async function uploadFile(file, type) {
    const formData = new FormData();

    let url;
    if (type === 'pypi') {
        formData.append('content', file);
        url = '/pypi/';
    } else if (type === 'npm') {
        // npm requires a specific JSON format, so we need to create a package structure
        // Sanitize package name to prevent injection
        let packageName = file.name.replace('.tgz', '').replace(/-\d+\.\d+\.\d+.*/, '');
        // Only allow alphanumeric characters, hyphens, underscores, and dots
        packageName = packageName.replace(/[^a-zA-Z0-9._-]/g, '');

        const packageJson = {
            name: packageName,
            version: '1.0.0',
            _attachments: {}
        };

        // Read file as base64
        const base64 = await fileToBase64(file);
        packageJson._attachments[file.name] = {
            content_type: 'application/octet-stream',
            data: base64,
            length: file.size
        };

        const response = await fetch(`/npm/${packageJson.name}`, {
            method: 'PUT',
            headers: {
                'Content-Type': 'application/json'
            },
            body: JSON.stringify(packageJson)
        });

        if (!response.ok) {
            const text = await response.text();
            throw new Error(text || response.statusText);
        }
        return;
    } else if (type === 'cargo') {
        formData.append('file', file);
        url = '/cargo/api/v1/crates/new';
    }

    if (type !== 'npm') {
        const response = await fetch(url, {
            method: 'POST',
            body: formData
        });

        if (!response.ok) {
            const text = await response.text();
            throw new Error(text || response.statusText);
        }
    }
}

// Convert file to base64
function fileToBase64(file) {
    return new Promise((resolve, reject) => {
        const reader = new FileReader();
        reader.readAsDataURL(file);
        reader.onload = () => {
            const base64 = reader.result.split(',')[1];
            resolve(base64);
        };
        reader.onerror = error => reject(error);
    });
}

// Format file size
function formatSize(size) {
    const units = ['B', 'KB', 'MB', 'GB'];
    let unitIdx = 0;
    let displaySize = size;

    while (displaySize >= 1024 && unitIdx < units.length - 1) {
        displaySize /= 1024;
        unitIdx++;
    }

    return unitIdx === 0
        ? `${displaySize} ${units[unitIdx]}`
        : `${displaySize.toFixed(1)} ${units[unitIdx]}`;
}