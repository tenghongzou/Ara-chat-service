// K6 Message Throughput Test for Ara Chat Service
// Tests maximum message processing capacity
//
// Usage:
//   k6 run --vus 100 --duration 10m message_throughput.js

import ws from 'k6/ws';
import { check, sleep } from 'k6';
import { Counter, Rate, Trend } from 'k6/metrics';
import { randomString, randomIntBetween } from 'https://jslib.k6.io/k6-utils/1.4.0/index.js';

// Metrics
const messagesSent = new Counter('messages_sent_total');
const messagesDelivered = new Counter('messages_delivered_total');
const messageLatency = new Trend('message_delivery_latency', true);
const sendErrors = new Counter('send_errors');
const deliveryRate = new Rate('message_delivery_rate');

const BASE_URL = __ENV.TARGET || 'ws://localhost:8082';

export const options = {
    scenarios: {
        // High throughput test
        high_throughput: {
            executor: 'constant-vus',
            vus: 100,
            duration: '10m',
        },
    },
    thresholds: {
        messages_sent_total: ['count>10000'],       // Send at least 10K messages
        message_delivery_rate: ['rate>0.99'],       // 99% delivery rate
        message_delivery_latency: ['p(99)<1000'],   // p99 < 1s
    },
};

// Simulates high-frequency message sender
export default function () {
    const userId = `throughput_user_${__VU}`;
    const conversationId = `throughput_conv_${__VU % 10}`; // 10 conversations

    const url = `${BASE_URL}/ws?token=test_token_${userId}`;
    const pendingMessages = new Map();

    ws.connect(url, {}, function (socket) {
        socket.on('open', () => {
            socket.send(JSON.stringify({
                type: 'Authenticate',
                payload: { token: `test_token_${userId}` }
            }));
        });

        socket.on('message', (data) => {
            try {
                const msg = JSON.parse(data);

                if (msg.type === 'message_sent' && msg.client_message_id) {
                    const sendTime = pendingMessages.get(msg.client_message_id);
                    if (sendTime) {
                        messageLatency.add(Date.now() - sendTime);
                        pendingMessages.delete(msg.client_message_id);
                        messagesDelivered.add(1);
                        deliveryRate.add(1);
                    }
                }
            } catch (e) {
                // Ignore parse errors
            }
        });

        socket.on('error', () => {
            sendErrors.add(1);
            deliveryRate.add(0);
        });

        // High-frequency message sending (10 messages per second per VU)
        socket.setInterval(() => {
            const clientMsgId = `${userId}_${Date.now()}_${randomString(4)}`;
            pendingMessages.set(clientMsgId, Date.now());

            socket.send(JSON.stringify({
                type: 'SendMessage',
                payload: {
                    conversation_id: conversationId,
                    content: `High throughput message ${clientMsgId}`,
                    content_type: 'Text',
                    client_message_id: clientMsgId,
                    mentions: [],
                }
            }));
            messagesSent.add(1);
        }, 100); // 10 messages/second

        // Run for 1 minute then reconnect
        socket.setTimeout(() => {
            socket.close();
        }, 60000);
    });

    sleep(1);
}

export function handleSummary(data) {
    const totalSent = data.metrics.messages_sent_total?.values?.count || 0;
    const totalDelivered = data.metrics.messages_delivered_total?.values?.count || 0;
    const avgLatency = data.metrics.message_delivery_latency?.values?.avg || 0;
    const p99Latency = data.metrics.message_delivery_latency?.values?.['p(99)'] || 0;

    return {
        'stdout': `
========================================
MESSAGE THROUGHPUT TEST RESULTS
========================================
Messages Sent:     ${totalSent}
Messages Delivered: ${totalDelivered}
Delivery Rate:     ${((totalDelivered / totalSent) * 100).toFixed(2)}%
Average Latency:   ${avgLatency.toFixed(2)}ms
P99 Latency:       ${p99Latency.toFixed(2)}ms
Throughput:        ${(totalSent / (data.state.testRunDurationMs / 1000)).toFixed(0)} msg/s
========================================
`,
        'results/throughput_summary.json': JSON.stringify(data, null, 2),
    };
}
