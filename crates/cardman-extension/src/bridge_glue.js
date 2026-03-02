// Bridge glue — wraps chrome.runtime.sendMessage in a Promise for wasm-bindgen.

export function sendMessage(msg) {
    return new Promise((resolve) => {
        chrome.runtime.sendMessage(msg, (response) => {
            resolve(response);
        });
    });
}
