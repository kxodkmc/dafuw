//! 游戏客户端：REST + WebSocket 的组合门面。
//!
//! 职责：
//! - 持有当前服务器地址与房间身份（会话令牌）
//! - 打开/关闭房间 WS，把服务器事件原样回调给上层
//! - 发送指令（serde 外部标签格式）
//! 不做任何状态推演，状态由 store 层负责。

import { createHttp } from './http.js';
import { createSocket } from './ws.js';
import { loadServer, saveServer, saveIdent, clearIdent, loadIdent } from '../state/session.js';

export class GameClient {
  /**
   * @param {{onEvent:(evt:object)=>void, onStatus:(connected:boolean, retrying:boolean)=>void}} hooks
   */
  constructor({ onEvent, onStatus }) {
    this.onEvent = onEvent;
    this.onStatus = onStatus;
    this.http = createHttp(loadServer());
    this.socket = null;
    /** @type {{roomCode:number, playerId:string, token:string, isOwner:boolean, ownerToken:string}|null} */
    this.ident = null;
  }

  /** 服务器地址（供界面回显）。 */
  get serverUrl() {
    return loadServer();
  }

  /** 切换服务器（大厅里修改地址后调用）。 */
  setServerUrl(url) {
    saveServer(url);
    this.http = createHttp(url);
  }

  /** 探测服务器可用性（GET /health）。 */
  probe() {
    return this.http.health();
  }

  /** 房间列表（局域网发现）。 */
  listRooms() {
    return this.http.listRooms();
  }

  /** 建房：返回 { room_code, owner_token }。 */
  createRoom(settings, name) {
    return this.http.createRoom(settings, name);
  }

  /** 入座：携带全局身份 {uid, secret}，返回 JoinRes（含座位与令牌）。 */
  joinRoom(code, identity) {
    return this.http.joinRoom(code, identity);
  }

  /**
   * 进入房间：保存身份并建立 WS。
   * @param {number} roomCode
   * @param {{playerId:string, token:string, isOwner:boolean, ownerToken?:string}} ident
   */
  enterRoom(roomCode, ident) {
    this.ident = {
      roomCode,
      playerId: ident.playerId,
      token: ident.token,
      isOwner: ident.isOwner,
      ownerToken: ident.ownerToken ?? '',
    };
    saveIdent(this.ident);
    this._openSocket(roomCode, ident.token);
  }

  /** 断线后用持久化身份重新入房（保留原令牌，不重新 join）。 */
  reconnectSaved() {
    return this.resumeRoom();
  }

  /** 用已保存的身份直接回到房间：保留原角色与房主权，不重新 join。 */
  resumeRoom() {
    const saved = loadIdent();
    if (!saved) return false;
    this.ident = saved;
    this._openSocket(saved.roomCode, saved.token);
    return true;
  }

  /**
   * 离开房间回到大厅。
   * 默认保留身份：房间与令牌仍有效，可随时「回到房间」恢复原角色；
   * 仅当房间已解散（RoomClosed）时才 forget 清除身份。
   */
  leaveRoom({ forget = false } = {}) {
    this.socket?.close();
    this.socket = null;
    this.ident = null;
    if (forget) clearIdent();
  }

  /**
   * 发送指令（外部标签对象）。
   * WS 握手中（CONNECTING）时自动排队，OPEN 后立即发出（5s 超时）；
   * 无连接或已关闭则抛错，由界面 toast 提示。
   */
  send(command) {
    const payload = JSON.stringify(command);
    const sock = this.socket;
    const fail = (why) => {
      console.warn(`[client] 发送失败：${why} readyState=${sock?.readyState} url=${sock?.url ?? '无'}`, command);
      throw new Error('与房间断开连接，正在自动重连…');
    };
    if (!sock) fail('无连接。');
    const rs = sock.readyState;
    if (rs === WebSocket.OPEN) {
      sock.send(payload);
      return;
    }
    if (rs === WebSocket.CONNECTING) {
      // 握手中：短暂排队，成功连接后立即发出，避免用户操作被吃掉
      const timer = setTimeout(() => {
        console.warn('[client] 排队指令等待连接超时（5s），已丢弃', command);
      }, 5000);
      sock.whenOpen(() => {
        clearTimeout(timer);
        sock.send(payload);
      });
      return;
    }
    fail('连接已关闭。');
  }

  /** 房主开局（HTTP）。 */
  startGame() {
    const { roomCode, ownerToken } = this._ownerIdent();
    return this.http.startGame(roomCode, ownerToken);
  }

  /** 房主解散房间（HTTP）。 */
  closeRoom() {
    const { roomCode, ownerToken } = this._ownerIdent();
    return this.http.closeRoom(roomCode, ownerToken);
  }

  _ownerIdent() {
    if (!this.ident?.isOwner || !this.ident.ownerToken) {
      throw new Error('仅房主可执行此操作');
    }
    return this.ident;
  }

  _openSocket(roomCode, token) {
    this.socket?.close();
    const url = `${this.http.wsUrl(`/ws/rooms/${roomCode}`)}?token=${encodeURIComponent(token)}`;
    this.socket = createSocket(url, {
      onMessage: (evt) => this.onEvent(evt),
      onStatus: (connected, retrying) => this.onStatus(connected, retrying),
    });
  }
}
