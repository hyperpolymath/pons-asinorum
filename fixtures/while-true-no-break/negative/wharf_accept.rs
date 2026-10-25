// SPDX-License-Identifier: MPL-2.0
async fn serve(listener: Listener) { loop { let connection = listener.accept().await; handle(connection); } }
