import {
  CloudDownloadOutlined,
  CloudSyncOutlined,
  CopyOutlined,
  DeleteOutlined,
  EyeOutlined,
  LeftOutlined,
  LinkOutlined,
  PlayCircleOutlined,
  ReloadOutlined,
  RightOutlined,
  StopOutlined,
  UploadOutlined,
} from "@ant-design/icons";
import { openUrl } from "@tauri-apps/plugin-opener";
import {
  Alert,
  Button,
  Card,
  Col,
  Collapse,
  Drawer,
  Input,
  Layout,
  Progress,
  Radio,
  Row,
  Space,
  Statistic,
  Table,
  Tag,
  Tooltip,
  Typography,
  Upload,
  message,
} from "antd";
import type { ColumnsType } from "antd/es/table";
import { useCallback, useEffect, useMemo, useState } from "react";
import {
  cancelScrape,
  downloadCsv,
  exportCsv,
  getProxy,
  initSession,
  listenScrapeProgress,
  parseSkus,
  refreshOne,
  runSelfCheck,
  setProxy,
  startScrape,
  testProxy,
} from "./api";
import "./App.css";
import type { ParseSkusResult, ProxyConfig, RowResult, ScrapeOptions } from "./types";
import { STATUS_COLORS, STATUS_LABELS } from "./types";

const { Header, Content } = Layout;
const { Title, Text } = Typography;
const { TextArea } = Input;

const CHUNK_SIZE = 50;

type ScrapeState = "idle" | "running" | "paused" | "done";

const DEFAULT_OPTIONS: ScrapeOptions = {
  requestIntervalMs: 1500,
};

const DEFAULT_PROXY: ProxyConfig = {
  mode: "auto",
};

const PROXY_MODE_LABELS: Record<ProxyConfig["mode"], string> = {
  auto: "自动",
  manual: "手动",
  off: "关闭",
};

function App() {
  const [inputText, setInputText] = useState("");
  const [rows, setRows] = useState<RowResult[]>([]);
  const [duplicateCount, setDuplicateCount] = useState(0);
  const [invalidCount, setInvalidCount] = useState(0);
  const [options] = useState<ScrapeOptions>(DEFAULT_OPTIONS);
  const [proxyConfig, setProxyConfig] = useState<ProxyConfig>(DEFAULT_PROXY);
  const [proxyTesting, setProxyTesting] = useState(false);
  const [proxySaving, setProxySaving] = useState(false);
  const [proxyPanelOpen, setProxyPanelOpen] = useState<string[]>([]);
  const [scrapeState, setScrapeState] = useState<ScrapeState>("idle");
  const [sessionMessage, setSessionMessage] = useState("正在初始化会话...");
  const [selfCheckMessage, setSelfCheckMessage] = useState<string | null>(null);
  const [reviewIndex, setReviewIndex] = useState<number | null>(null);
  const [viewed, setViewed] = useState<Set<string>>(() => new Set());

  const markViewed = useCallback((asin: string) => {
    setViewed((current) => {
      const next = new Set(current);
      next.add(asin);
      return next;
    });
  }, []);

  const clearViewed = useCallback(() => {
    setViewed(new Set());
    message.info("已清空全部已查看记录");
  }, []);

  const unmarkViewed = useCallback((asin: string) => {
    setViewed((current) => {
      const next = new Set(current);
      next.delete(asin);
      return next;
    });
  }, []);

  const resetViewedForRescrape = useCallback(() => {
    setViewed(new Set());
  }, []);

  useEffect(() => {
    let unlisten: (() => void) | undefined;

    const bootstrap = async () => {
      try {
        const savedProxy = await getProxy();
        setProxyConfig(savedProxy);

        setSessionMessage("正在设置日语与日本配送地区（可能需要 10–30 秒）...");
        const session = await initSession();
        setSessionMessage(session.message);
        const check = await runSelfCheck();
        setSelfCheckMessage(check.ok ? "自检通过" : check.message);
      } catch (error) {
        setSessionMessage(`会话初始化失败: ${String(error)}`);
      }

      unlisten = await listenScrapeProgress((payload) => {
        setRows((current) =>
          current.map((row) =>
            row.asin === payload.row.asin ? payload.row : row,
          ),
        );
      });
    };

    void bootstrap();
    return () => {
      unlisten?.();
    };
  }, []);

  const reviewRow = reviewIndex != null ? rows[reviewIndex] ?? null : null;
  const manualProxy = proxyConfig.mode === "manual";
  const proxyPanelLabel = useMemo(() => {
    const modeLabel = PROXY_MODE_LABELS[proxyConfig.mode];
    if (manualProxy && proxyConfig.url?.trim()) {
      return `代理设置 · ${modeLabel} · ${proxyConfig.url.trim()}`;
    }
    return `代理设置 · ${modeLabel}`;
  }, [manualProxy, proxyConfig.mode, proxyConfig.url]);

  const validCount = useMemo(
    () => rows.filter((row) => row.status !== "formatError").length,
    [rows],
  );

  const pendingCount = useMemo(
    () =>
      rows.filter(
        (row) => row.status !== "formatError" && row.status === "pending",
      ).length,
    [rows],
  );

  const progressDone = useMemo(
    () =>
      rows.filter(
        (row) => row.status !== "formatError" && row.status !== "pending",
      ).length,
    [rows],
  );

  const controlsLocked = scrapeState === "running";

  const scrapeStatusMessage = useMemo(() => {
    if (scrapeState === "running") {
      return "抓取中…";
    }
    if (scrapeState === "paused" && pendingCount > 0) {
      return `已暂停，还剩 ${pendingCount} 条待抓取`;
    }
    if (scrapeState === "done") {
      return "抓取已完成";
    }
    return null;
  }, [scrapeState, pendingCount]);

  const successCount = useMemo(
    () => rows.filter((row) => row.status === "success").length,
    [rows],
  );

  const failedCount = useMemo(
    () =>
      rows.filter((row) =>
        ["failed", "unavailable", "noPrice", "mismatch"].includes(row.status),
      ).length,
    [rows],
  );

  const openAmazonLink = async (record: RowResult) => {
    markViewed(record.asin);
    try {
      await openUrl(record.amazonUrl);
    } catch (error) {
      message.error(`打开链接失败: ${String(error)}`);
    }
  };

  const resetRowsToPending = useCallback((current: RowResult[]) => {
    return current.map((row) =>
      row.status === "formatError"
        ? row
        : {
            ...row,
            status: "pending" as const,
            priceText: null,
            priceValue: null,
            error: null,
            fetchedAt: null,
          },
    );
  }, []);

  const runNextChunk = useCallback(
    async (sourceRows: RowResult[]) => {
      const pending = sourceRows.filter(
        (row) => row.status !== "formatError" && row.status === "pending",
      );
      const chunk = pending.slice(0, CHUNK_SIZE);
      if (chunk.length === 0) {
        setScrapeState("done");
        message.success("抓取完成");
        return;
      }

      setScrapeState("running");
      try {
        const session = await initSession();
        setSessionMessage(session.message);
        const results = await startScrape(chunk, options);
        const updated = sourceRows.map((row) => {
          const patch = results.find((result) => result.asin === row.asin);
          return patch ?? row;
        });
        const remaining = updated.filter(
          (row) => row.status !== "formatError" && row.status === "pending",
        ).length;
        setRows(updated);
        if (remaining > 0) {
          setScrapeState("paused");
          message.info(`本片完成，还剩 ${remaining} 条待抓取`);
        } else {
          setScrapeState("done");
          message.success("抓取完成");
        }
      } catch (error) {
        setScrapeState("paused");
        message.error(`抓取失败: ${String(error)}`);
      }
    },
    [options],
  );

  const applyParseResult = (result: ParseSkusResult, source: "parse" | "upload") => {
    setRows(result.rows);
    setDuplicateCount(result.duplicateCount);
    setInvalidCount(result.invalidCount);
    setScrapeState("idle");
    setReviewIndex(null);

    if (result.validCount === 0) {
      message.error(
        "没有有效的 SKU：去掉可选 gx- 后应为 10 位 ASIN，或 13 位且末 3 位为数字后缀",
      );
      return;
    }

    const parts = [`有效 ${result.validCount} 条`];
    if (result.duplicateCount > 0) {
      parts.push(`去重 ${result.duplicateCount} 条`);
    }
    if (result.invalidCount > 0) {
      parts.push(`格式错误 ${result.invalidCount} 条`);
    }

    if (result.invalidCount > 0) {
      message.warning(`${parts.join("，")}；格式错误的行不会参与抓取`);
    } else if (source === "upload") {
      message.success(`已从文件读取：${parts.join("，")}`);
    } else {
      message.success(`识别完成：${parts.join("，")}`);
    }
  };

  const handleParse = async () => {
    const result = await parseSkus(inputText);
    applyParseResult(result, "parse");
  };

  const handleUpload = async (file: File) => {
    const text = await file.text();
    setInputText(text);
    const result = await parseSkus(text);
    applyParseResult(result, "upload");
    return false;
  };

  const handleStart = async () => {
    if (rows.length === 0) {
      message.warning("请先输入或上传 SKU");
      return;
    }
    if (validCount === 0) {
      message.warning("没有可抓取的有效 SKU，请修正格式错误后重试");
      return;
    }

    resetViewedForRescrape();
    const resetRows = resetRowsToPending(rows);
    setRows(resetRows);
    await runNextChunk(resetRows);
  };

  const handleContinue = async () => {
    await runNextChunk(rows);
  };

  const handlePause = async () => {
    await cancelScrape();
    message.info("暂停请求已发送，当前条完成后停止");
  };

  const handleRefreshAll = async () => {
    if (rows.length === 0) {
      return;
    }
    resetViewedForRescrape();
    const resetRows = resetRowsToPending(rows);
    setRows(resetRows);
    await runNextChunk(resetRows);
  };

  const handleExport = async () => {
    if (rows.length === 0) {
      message.warning("没有可导出的数据");
      return;
    }
    const csv = await exportCsv(rows);
    downloadCsv(csv);
    message.success("CSV 已导出");
  };

  const handleTestProxy = async () => {
    setProxyTesting(true);
    try {
      const result = await testProxy(proxyConfig);
      if (result.ok) {
        message.success(result.message);
      } else {
        message.error(result.message);
      }
    } catch (error) {
      message.error(`代理测试失败: ${String(error)}`);
    } finally {
      setProxyTesting(false);
    }
  };

  const handleSaveProxy = async () => {
    setProxySaving(true);
    try {
      const saved = await setProxy(proxyConfig);
      setProxyConfig(saved);
      message.success("代理已保存，会话将按新配置重建");
      setProxyPanelOpen([]);

      setSessionMessage("正在按新代理重建会话...");
      const session = await initSession();
      setSessionMessage(session.message);
      const check = await runSelfCheck();
      setSelfCheckMessage(check.ok ? "自检通过" : check.message);
    } catch (error) {
      message.error(`保存代理失败: ${String(error)}`);
    } finally {
      setProxySaving(false);
    }
  };

  const copyPriceValue = async (record: RowResult) => {
    let value = "";
    if (record.priceText && /\./.test(record.priceText)) {
      value = record.priceText.replace(/[^\d.]/g, "");
    } else if (record.priceValue != null) {
      value = record.priceValue.toString();
    } else if (record.priceText) {
      value = record.priceText.replace(/[^\d.]/g, "");
    }
    if (!value) {
      message.warning("暂无可复制的价格");
      return;
    }
    try {
      await navigator.clipboard.writeText(value);
      message.success(`已复制 ${value}`);
    } catch {
      message.error("复制失败");
    }
  };

  const openReview = (index: number) => {
    setReviewIndex(index);
  };

  const columns: ColumnsType<RowResult> = [
    { title: "SKU", dataIndex: "sku", key: "sku", width: 180, ellipsis: true },
    { title: "dp code", dataIndex: "dpCode", key: "dpCode", width: 120 },
    { title: "ASIN", dataIndex: "asin", key: "asin", width: 120 },
    {
      title: "价格 (JPY)",
      dataIndex: "priceText",
      key: "priceText",
      width: 140,
      render: (value, record) => {
        if (!value) {
          return "-";
        }
        return (
          <Space size={4}>
            <span>{value}</span>
            <Tooltip title="复制数值">
              <Button
                type="text"
                size="small"
                icon={<CopyOutlined />}
                className="copy-price-btn"
                onClick={() => void copyPriceValue(record)}
              />
            </Tooltip>
          </Space>
        );
      },
    },
    {
      title: "Amazon 链接",
      dataIndex: "amazonUrl",
      key: "amazonUrl",
      width: 120,
      render: (_value, record) => (
        <Button
          type="link"
          size="small"
          icon={<LinkOutlined />}
          onClick={() => void openAmazonLink(record)}
        >
          搜索页
        </Button>
      ),
    },
    {
      title: "状态",
      dataIndex: "status",
      key: "status",
      width: 130,
      render: (status: RowResult["status"], record) => (
        <Space size={4} wrap>
          <Tag color={STATUS_COLORS[status]}>{STATUS_LABELS[status]}</Tag>
          {viewed.has(record.asin) ? <Tag color="default">✓ 已看</Tag> : null}
        </Space>
      ),
    },
    {
      title: "抓取时间",
      dataIndex: "fetchedAt",
      key: "fetchedAt",
      width: 180,
      render: (value) => value ?? "-",
    },
    {
      title: "操作",
      key: "actions",
      fixed: "right",
      width: 170,
      render: (_, record, index) => (
        <Space size={4}>
          <Button
            size="small"
            icon={<EyeOutlined />}
            onClick={() => openReview(index)}
          >
            查看
          </Button>
          <Button
            size="small"
            icon={<ReloadOutlined />}
            disabled={controlsLocked || record.status === "formatError"}
            onClick={async () => {
              setScrapeState("running");
              try {
                const updated = await refreshOne(record, options);
                setRows((current) =>
                  current.map((row) => (row.asin === updated.asin ? updated : row)),
                );
              } catch (error) {
                message.error(String(error));
              } finally {
                setScrapeState("idle");
              }
            }}
          >
            刷新
          </Button>
          {viewed.has(record.asin) ? (
            <Tooltip title="删除已查看记录">
              <Button
                size="small"
                icon={<DeleteOutlined />}
                onClick={() => unmarkViewed(record.asin)}
              />
            </Tooltip>
          ) : null}
        </Space>
      ),
    },
  ];

  return (
    <Layout className="app-layout">
      <Header className="app-header">
        <div>
          <Title level={3} style={{ color: "#fff", margin: 0 }}>
            Amazon 价格抓取
          </Title>
          <Text style={{ color: "rgba(255,255,255,0.75)" }}>
            批量解析 SKU，抓取 Amazon.co.jp 搜索页价格
          </Text>
        </div>
      </Header>

      <Content className="app-content">
        <Space direction="vertical" size="large" style={{ width: "100%" }}>
          <Alert
            className="session-alert"
            type={selfCheckMessage?.includes("通过") ? "success" : "info"}
            showIcon
            message={
              scrapeStatusMessage ??
              (selfCheckMessage
                ? `${sessionMessage} · ${selfCheckMessage}`
                : sessionMessage)
            }
          />

          <Row gutter={[16, 16]}>
            <Col xs={24} xl={10}>
              <Card title="SKU 输入" className="panel-card">
                <Space direction="vertical" style={{ width: "100%" }} size="middle">
                  <TextArea
                    rows={10}
                    value={inputText}
                    placeholder={
                      "每行一个 SKU，gx- 前缀与 3 位后缀均可选，例如:\n" +
                      "gx-b0dfxqwpps149\n" +
                      "b0dfxqwpps\n" +
                      "B08CKGRHLF"
                    }
                    onChange={(e) => setInputText(e.target.value)}
                  />
                  <Space wrap>
                    <Upload
                      beforeUpload={handleUpload}
                      showUploadList={false}
                      accept=".txt"
                      disabled={controlsLocked}
                    >
                      <Button icon={<UploadOutlined />} disabled={controlsLocked}>
                        上传 txt
                      </Button>
                    </Upload>
                    <Button
                      type="primary"
                      className="parse-sku-btn"
                      disabled={controlsLocked}
                      onClick={() => void handleParse()}
                    >
                      解析 SKU
                    </Button>
                  </Space>
                  <Text type="secondary">
                    上传 txt 会自动解析；手动粘贴后请点击「解析 SKU」
                  </Text>
                  <Text type="secondary">
                    已识别 {rows.length} 条，有效 {validCount} 条
                    {invalidCount > 0 ? `，格式错误 ${invalidCount} 条` : ""}
                    {duplicateCount > 0 ? `，去重 ${duplicateCount} 条` : ""}
                  </Text>
                </Space>
              </Card>
            </Col>

            <Col xs={24} xl={14}>
              <Card title="抓取控制" className="panel-card">
                <Space direction="vertical" style={{ width: "100%" }} size="middle">
                  <Collapse
                    activeKey={proxyPanelOpen}
                    onChange={(keys) => setProxyPanelOpen(keys as string[])}
                    items={[
                      {
                        key: "proxy",
                        label: proxyPanelLabel,
                        children: (
                          <Space direction="vertical" style={{ width: "100%" }} size="middle">
                            <Radio.Group
                              value={proxyConfig.mode}
                              onChange={(e) =>
                                setProxyConfig((current) => ({
                                  ...current,
                                  mode: e.target.value,
                                }))
                              }
                            >
                              <Space direction="vertical">
                                <Radio value="auto">自动（系统/环境代理）</Radio>
                                <Radio value="manual">手动</Radio>
                                <Radio value="off">关闭（直连）</Radio>
                              </Space>
                            </Radio.Group>
                            <Row gutter={[12, 12]}>
                              <Col span={24}>
                                <Input
                                  placeholder="http://127.0.0.1:7890 或 socks5://127.0.0.1:7891"
                                  value={proxyConfig.url ?? ""}
                                  disabled={!manualProxy}
                                  onChange={(e) =>
                                    setProxyConfig((current) => ({
                                      ...current,
                                      url: e.target.value,
                                    }))
                                  }
                                />
                              </Col>
                              <Col xs={12}>
                                <Input
                                  placeholder="用户名（可选）"
                                  value={proxyConfig.username ?? ""}
                                  disabled={!manualProxy}
                                  onChange={(e) =>
                                    setProxyConfig((current) => ({
                                      ...current,
                                      username: e.target.value,
                                    }))
                                  }
                                />
                              </Col>
                              <Col xs={12}>
                                <Input.Password
                                  placeholder="密码（可选）"
                                  value={proxyConfig.password ?? ""}
                                  disabled={!manualProxy}
                                  onChange={(e) =>
                                    setProxyConfig((current) => ({
                                      ...current,
                                      password: e.target.value,
                                    }))
                                  }
                                />
                              </Col>
                            </Row>
                            <Space wrap>
                              <Button
                                loading={proxyTesting}
                                onClick={() => void handleTestProxy()}
                              >
                                测试代理
                              </Button>
                              <Button
                                type="primary"
                                loading={proxySaving}
                                onClick={() => void handleSaveProxy()}
                              >
                                保存代理
                              </Button>
                            </Space>
                          </Space>
                        ),
                      },
                    ]}
                  />

                  <Row gutter={[16, 16]}>
                    <Col span={24}>
                      <Text>抓取间隔</Text>
                      <div>
                        <Text type="secondary">每 1.5 秒 1 条商品，每 {CHUNK_SIZE} 条自动暂停</Text>
                      </div>
                    </Col>
                  </Row>

                  <Space wrap>
                    {(scrapeState === "idle" || scrapeState === "done") && (
                      <Button
                        type="primary"
                        icon={<PlayCircleOutlined />}
                        disabled={controlsLocked || rows.length === 0}
                        onClick={() => void handleStart()}
                      >
                        开始抓取
                      </Button>
                    )}
                    {scrapeState === "paused" && (
                      <Button
                        type="primary"
                        icon={<PlayCircleOutlined />}
                        onClick={() => void handleContinue()}
                      >
                        继续抓取（剩 {pendingCount} 条）
                      </Button>
                    )}
                    {scrapeState === "running" && (
                      <Button icon={<StopOutlined />} onClick={() => void handlePause()}>
                        暂停
                      </Button>
                    )}
                    <Button
                      icon={<CloudSyncOutlined />}
                      disabled={controlsLocked || rows.length === 0}
                      onClick={() => void handleRefreshAll()}
                    >
                      全部刷新
                    </Button>
                    <Button
                      icon={<CloudDownloadOutlined />}
                      disabled={rows.length === 0}
                      onClick={() => void handleExport()}
                    >
                      导出 CSV
                    </Button>
                  </Space>

                  <div>
                    <Progress
                      percent={
                        validCount > 0
                          ? Math.round((progressDone / validCount) * 100)
                          : 0
                      }
                      status={scrapeState === "running" ? "active" : "normal"}
                    />
                    <Row gutter={16} style={{ marginTop: 12 }}>
                      <Col span={8}>
                        <Statistic title="完成" value={progressDone} suffix={`/ ${validCount}`} />
                      </Col>
                      <Col span={8}>
                        <Statistic title="成功" value={successCount} />
                      </Col>
                      <Col span={8}>
                        <Statistic title="异常" value={failedCount} />
                      </Col>
                    </Row>
                  </div>
                </Space>
              </Card>
            </Col>
          </Row>

          <Card
            title="结果列表"
            className="panel-card"
            extra={
              <Button size="small" disabled={viewed.size === 0} onClick={clearViewed}>
                清空已查看
              </Button>
            }
          >
            <Table
              rowKey="asin"
              columns={columns}
              dataSource={rows}
              pagination={{ pageSize: 10, showSizeChanger: true }}
              scroll={{ x: 1200 }}
              size="middle"
              rowClassName={(record) =>
                viewed.has(record.asin) ? "row-viewed" : ""
              }
              onRow={(_, index) => ({
                onDoubleClick: () => {
                  if (index != null) {
                    openReview(index);
                  }
                },
              })}
            />
          </Card>
        </Space>
      </Content>

      <Drawer
        title="核价详情"
        open={reviewRow != null}
        width={480}
        onClose={() => setReviewIndex(null)}
        extra={
          reviewRow ? (
            <Space>
              <Button
                icon={<LeftOutlined />}
                disabled={reviewIndex == null || reviewIndex <= 0}
                onClick={() =>
                  setReviewIndex((current) =>
                    current == null ? current : Math.max(0, current - 1),
                  )
                }
              >
                上一条
              </Button>
              <Button
                icon={<RightOutlined />}
                disabled={
                  reviewIndex == null || reviewIndex >= rows.length - 1
                }
                onClick={() =>
                  setReviewIndex((current) =>
                    current == null
                      ? current
                      : Math.min(rows.length - 1, current + 1),
                  )
                }
              >
                下一条
              </Button>
            </Space>
          ) : null
        }
      >
        {reviewRow ? (
          <Space direction="vertical" size="middle" style={{ width: "100%" }}>
            <div>
              <Text type="secondary">SKU</Text>
              <div>{reviewRow.sku}</div>
            </div>
            <div>
              <Text type="secondary">ASIN</Text>
              <div>{reviewRow.asin}</div>
            </div>
            <div>
              <Text type="secondary">dp code</Text>
              <div>{reviewRow.dpCode}</div>
            </div>
            <div>
              <Text type="secondary">价格</Text>
              <div>{reviewRow.priceText ?? "-"}</div>
            </div>
            <div>
              <Text type="secondary">状态</Text>
              <div>
                <Space>
                  <Tag color={STATUS_COLORS[reviewRow.status]}>
                    {STATUS_LABELS[reviewRow.status]}
                  </Tag>
                  {viewed.has(reviewRow.asin) ? (
                    <Tag color="default">✓ 已看</Tag>
                  ) : null}
                </Space>
              </div>
            </div>
            {reviewRow.error ? (
              <div>
                <Text type="secondary">错误</Text>
                <div>{reviewRow.error}</div>
              </div>
            ) : null}
            <Space wrap>
              <Button
                type="primary"
                icon={<LinkOutlined />}
                onClick={() => void openAmazonLink(reviewRow)}
              >
                打开搜索页
              </Button>
              {viewed.has(reviewRow.asin) ? (
                <Button
                  icon={<DeleteOutlined />}
                  onClick={() => unmarkViewed(reviewRow.asin)}
                >
                  删除记录
                </Button>
              ) : null}
            </Space>
            {reviewIndex != null ? (
              <Text type="secondary">
                第 {reviewIndex + 1} / {rows.length} 条
              </Text>
            ) : null}
          </Space>
        ) : null}
      </Drawer>
    </Layout>
  );
}

export default App;
